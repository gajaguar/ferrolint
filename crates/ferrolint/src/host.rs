//! The WASM plugin host: compile and link each plugin once, then check
//! every file in a fresh instance under an empty WASI environment with a
//! fuel budget, an epoch deadline and a memory cap.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

use ferrolint_core::{RuleInfo, Severity};
use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{
    Engine, InstanceAllocationStrategy, PoolingAllocationConfig, Store, StoreLimits,
    StoreLimitsBuilder, Trap,
};
use wasmtime_wasi::{
    Deterministic, HostMonotonicClock, HostWallClock, WasiCtx, WasiCtxBuilder, WasiCtxView,
    WasiView,
};

use crate::ast::Flat;
use crate::cache::{Cache, PluginMeta};
use crate::error::{Error, Result};
use crate::semantic::Model;
use crate::wit::{self, Binding, Diagnostic, File, Plugin, PluginPre, Scope};

/// Instructions one plugin may run on one file.
const FUEL_PER_FILE: u64 = 2_000_000_000;
/// How often the epoch advances.
const EPOCH_TICK: Duration = Duration::from_millis(10);
/// Epoch ticks one plugin may take on one file: ten seconds.
const EPOCHS_PER_FILE: u64 = 1_000;
/// Bytes of memory and tables a recycled instance slot keeps resident.
const KEEP_RESIDENT: usize = 4 << 20;
/// Linear memory one plugin instance may grow to.
const MEMORY_LIMIT: usize = 512 << 20;

/// A file as the plugins see it: its node list and semantic model.
pub struct FileData {
    pub flat: Flat,
    /// The packed form of `flat` the plugins receive.
    pub ast: wit::Ast,
    pub model: Model,
}

/// Per-store host state.
pub struct State {
    wasi: WasiCtx,
    table: ResourceTable,
    limits: StoreLimits,
    file: Option<Arc<FileData>>,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl State {
    fn new() -> Self {
        // An empty environment: no preopens, variables, arguments or
        // network; stdin closed and output discarded; clocks frozen and
        // randomness fixed, so a plugin learns nothing about the machine
        // and its output depends on its input alone.
        let wasi = WasiCtxBuilder::new()
            .wall_clock(FrozenClock)
            .monotonic_clock(FrozenClock)
            .secure_random(Deterministic::new(vec![0x5a]))
            .insecure_random(Deterministic::new(vec![0xa5]))
            .insecure_random_seed(0)
            .allow_tcp(false)
            .allow_udp(false)
            .allow_ip_name_lookup(false)
            .build();
        Self {
            wasi,
            table: ResourceTable::new(),
            limits: StoreLimitsBuilder::new().memory_size(MEMORY_LIMIT).build(),
            file: None,
        }
    }

    fn file(&self) -> wasmtime::Result<&FileData> {
        self.file
            .as_deref()
            .ok_or_else(|| wasmtime::Error::msg("semantic query outside a check call"))
    }

    fn check_node(&self, node: u32) -> wasmtime::Result<&FileData> {
        let file = self.file()?;
        if (node as usize) < file.flat.nodes.len() {
            Ok(file)
        } else {
            Err(wasmtime::Error::msg(format!(
                "semantic query for unknown node {node}"
            )))
        }
    }
}

struct FrozenClock;

impl HostWallClock for FrozenClock {
    fn resolution(&self) -> Duration {
        Duration::from_secs(1)
    }

    fn now(&self) -> Duration {
        Duration::ZERO
    }
}

impl HostMonotonicClock for FrozenClock {
    fn resolution(&self) -> u64 {
        1_000_000_000
    }

    fn now(&self) -> u64 {
        0
    }
}

impl wit::ferrolint::plugin::types::Host for State {}

impl wit::ferrolint::plugin::semantic::Host for State {
    fn scope_of(&mut self, node: u32) -> wasmtime::Result<Scope> {
        let file = self.check_node(node)?;
        file.model
            .scope(node)
            .ok_or_else(|| wasmtime::Error::msg(format!("no scope for node {node}")))
    }

    fn visible_bindings(&mut self, node: u32) -> wasmtime::Result<Vec<Binding>> {
        let file = self.check_node(node)?;
        Ok(file.model.visible_bindings(&file.flat, node))
    }

    fn resolve(&mut self, node: u32) -> wasmtime::Result<Option<Binding>> {
        let file = self.check_node(node)?;
        Ok(file.model.resolve(&file.flat, node))
    }
}

/// A plugin file read and checked against its pinned SHA-256.
pub struct PluginSource {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub sha256: String,
}

/// A plugin whose rules are known. It compiles on first use when its
/// rules came from the cache, so a fully warm run compiles nothing.
pub struct LoadedPlugin {
    pub name: String,
    pub rules: Vec<RuleInfo>,
    pub sha256: String,
    source: PluginSource,
    pre: OnceLock<std::result::Result<PluginPre<State>, String>>,
}

/// How a check call ended.
pub enum Outcome {
    Diagnostics(Vec<Diagnostic>),
    /// The plugin trapped or hit a cap; the reason says which.
    Aborted(String),
    /// The plugin, loaded from cached metadata, failed to compile or link:
    /// a load error found late, which stops the run.
    Broken(String),
}

/// One instance of a plugin, for one call.
struct Live {
    store: Store<State>,
    plugin: Plugin,
}

pub struct Host {
    engine: Engine,
    linker: Linker<State>,
    pub plugins: Vec<LoadedPlugin>,
    ticker: OnceLock<Ticker>,
    compiles: AtomicUsize,
}

impl Host {
    /// Link the contract and learn each plugin's rules: from the cache when
    /// it knows the plugin's SHA-256, otherwise by compiling the plugin and
    /// asking it. `plugins` are in run order.
    /// `workers` is how many threads check files at once, which sizes the
    /// pool of instance slots.
    pub fn load(plugins: Vec<PluginSource>, cache: Option<&Cache>, workers: usize) -> Result<Self> {
        let engine = engine(workers)?;
        let mut linker = Linker::<State>::new(&engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
            .and_then(|()| {
                Plugin::add_to_linker::<State, HasSelf<State>>(&mut linker, |state| state)
            })
            .map_err(|err| Error::plugin(format!("cannot link the plugin contract: {err}")))?;
        let compiles = AtomicUsize::new(0);

        let loaded: Vec<Result<LoadedPlugin>> = std::thread::scope(|scope| {
            let handles: Vec<_> = plugins
                .into_iter()
                .map(|source| {
                    let (engine, linker, compiles) = (&engine, &linker, &compiles);
                    scope.spawn(move || {
                        if let Some(meta) = cache.and_then(|cache| cache.plugin(&source.sha256)) {
                            return Ok(LoadedPlugin {
                                name: meta.name,
                                rules: meta.rules,
                                sha256: source.sha256.clone(),
                                source,
                                pre: OnceLock::new(),
                            });
                        }
                        compiles.fetch_add(1, Ordering::Relaxed);
                        let plugin = load_one(engine, linker, source)?;
                        if let Some(cache) = cache {
                            let meta = PluginMeta {
                                name: plugin.name.clone(),
                                rules: plugin.rules.clone(),
                            };
                            cache.store_plugin(&plugin.sha256, &meta);
                        }
                        Ok(plugin)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or_else(|_| Err(Error::plugin("plugin loading panicked")))
                })
                .collect()
        });
        let plugins = loaded.into_iter().collect::<Result<Vec<_>>>()?;
        Ok(Self {
            engine,
            linker,
            plugins,
            ticker: OnceLock::new(),
            compiles,
        })
    }

    /// How many plugins this run compiled.
    pub fn compiles(&self) -> usize {
        self.compiles.load(Ordering::Relaxed)
    }

    /// The plugin's pre-linked component, compiling it on first use.
    fn pre<'a>(
        &self,
        loaded: &'a LoadedPlugin,
    ) -> std::result::Result<&'a PluginPre<State>, String> {
        loaded
            .pre
            .get_or_init(|| {
                self.compiles.fetch_add(1, Ordering::Relaxed);
                link(&self.engine, &self.linker, &loaded.source).map_err(|err| err.message)
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    /// Run one plugin's check on one file in a fresh instance, so nothing a
    /// plugin keeps between calls can make a file's result depend on which
    /// files the same thread checked before it.
    pub fn check(&self, plugin: usize, file: &File, data: &Arc<FileData>) -> Outcome {
        let loaded = &self.plugins[plugin];
        let pre = match self.pre(loaded) {
            Ok(pre) => pre,
            Err(reason) => return Outcome::Broken(reason),
        };
        self.ticker
            .get_or_init(|| Ticker::start(self.engine.clone()));
        let mut instance = match instantiate(&self.engine, pre) {
            Ok(instance) => instance,
            Err(err) => return Outcome::Aborted(describe(&err)),
        };
        instance.store.data_mut().file = Some(Arc::clone(data));
        let result = arm(&mut instance.store).and_then(|()| {
            instance.plugin.ferrolint_plugin_checker().call_check(
                &mut instance.store,
                file,
                &data.ast,
            )
        });
        match result {
            Ok(diagnostics) => Outcome::Diagnostics(diagnostics),
            Err(err) => Outcome::Aborted(describe(&err)),
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        if let Some(ticker) = self.ticker.take() {
            ticker.stop();
        }
    }
}

/// The engine: component model, fuel and epochs, and a pooling allocator
/// that recycles instance slots, since every file gets fresh instances.
/// Without the address space for the pool, it falls back to allocating
/// each instance on demand.
fn engine(workers: usize) -> Result<Engine> {
    let mut config = wasmtime::Config::new();
    config
        .wasm_component_model(true)
        .consume_fuel(true)
        .epoch_interruption(true);
    let slots = u32::try_from(workers.saturating_mul(2).max(8)).unwrap_or(u32::MAX);
    let mut pool = PoolingAllocationConfig::new();
    pool.total_component_instances(slots)
        .total_core_instances(slots.saturating_mul(16))
        .total_memories(slots.saturating_mul(4))
        .total_tables(slots.saturating_mul(16))
        .total_stacks(slots)
        .max_memories_per_component(4)
        .max_tables_per_component(16)
        .max_memory_size(MEMORY_LIMIT)
        // Reset a recycled slot's first pages in place instead of handing
        // them back to the kernel, so the next file does not fault them in.
        .linear_memory_keep_resident(KEEP_RESIDENT)
        .table_keep_resident(KEEP_RESIDENT);
    let mut pooled = config.clone();
    pooled.allocation_strategy(InstanceAllocationStrategy::Pooling(pool));
    Engine::new(&pooled)
        .or_else(|_| Engine::new(&config))
        .map_err(|err| Error::plugin(format!("cannot start the WASM engine: {err}")))
}

/// Compile and link a plugin against the contract.
fn link(
    engine: &Engine,
    linker: &Linker<State>,
    source: &PluginSource,
) -> Result<PluginPre<State>> {
    let fail = |what: &str, err: &dyn std::fmt::Display| {
        Error::plugin(format!("{}: {what}: {err:#}", source.path.display()))
    };
    let component =
        Component::new(engine, &source.bytes).map_err(|err| fail("not a WASM component", &err))?;
    let pre = linker
        .instantiate_pre(&component)
        .map_err(|err| fail("imports outside the plugin contract", &err))?;
    PluginPre::new(pre).map_err(|err| fail("does not implement the plugin contract", &err))
}

/// Compile a plugin and ask it for its rules.
fn load_one(engine: &Engine, linker: &Linker<State>, source: PluginSource) -> Result<LoadedPlugin> {
    let pre = link(engine, linker, &source)?;
    let fail = |what: &str, err: &dyn std::fmt::Display| {
        Error::plugin(format!("{}: {what}: {err:#}", source.path.display()))
    };
    let mut instance = instantiate(engine, &pre).map_err(|err| fail("cannot instantiate", &err))?;
    let info = arm(&mut instance.store)
        .and_then(|()| {
            instance
                .plugin
                .ferrolint_plugin_checker()
                .call_info(&mut instance.store)
        })
        .map_err(|err| fail("info() failed", &describe(&err)))?;
    let rules = rules(&source.path, &info)?;
    Ok(LoadedPlugin {
        name: info.name,
        rules,
        sha256: source.sha256.clone(),
        source,
        pre: OnceLock::from(Ok(pre)),
    })
}

fn rules(path: &Path, info: &wit::PluginInfo) -> Result<Vec<RuleInfo>> {
    if info.name.trim().is_empty() {
        return Err(Error::plugin(format!(
            "{}: info() names no plugin",
            path.display()
        )));
    }
    info.rules
        .iter()
        .map(|rule| {
            if rule.code.trim().is_empty() || rule.code.contains(char::is_whitespace) {
                return Err(Error::plugin(format!(
                    "{}: plugin {} declares the invalid rule code {:?}",
                    path.display(),
                    info.name,
                    rule.code
                )));
            }
            Ok(RuleInfo {
                code: rule.code.clone(),
                name: rule.name.clone(),
                summary: rule.summary.clone(),
                severity: severity(rule.severity),
                pylint_id: rule.pylint_id.clone(),
                pylint_symbol: rule.pylint_symbol.clone(),
                plugin: Some(info.name.clone()),
            })
        })
        .collect()
}

pub fn severity(severity: wit::Severity) -> Severity {
    match severity {
        wit::Severity::Error => Severity::Error,
        wit::Severity::Warning => Severity::Warning,
        wit::Severity::Info => Severity::Info,
    }
}

fn instantiate(engine: &Engine, pre: &PluginPre<State>) -> wasmtime::Result<Live> {
    let mut store = Store::new(engine, State::new());
    store.limiter(|state| &mut state.limits);
    arm(&mut store)?;
    let plugin = pre.instantiate(&mut store)?;
    Ok(Live { store, plugin })
}

/// Refill the per-call budgets: fuel and the epoch deadline.
fn arm(store: &mut Store<State>) -> wasmtime::Result<()> {
    store.set_fuel(FUEL_PER_FILE)?;
    store.set_epoch_deadline(EPOCHS_PER_FILE);
    Ok(())
}

/// Why a call failed, in words a report can carry.
fn describe(err: &wasmtime::Error) -> String {
    if let Some(trap) = err.downcast_ref::<Trap>() {
        return match trap {
            Trap::OutOfFuel => "ran out of fuel (instruction budget)".to_owned(),
            Trap::Interrupt => "exceeded its time budget".to_owned(),
            other => format!("trapped: {other}"),
        };
    }
    if let Some(exit) = err.downcast_ref::<wasmtime_wasi::I32Exit>() {
        return format!("called the disabled builtin exit({})", exit.0);
    }
    format!("failed: {err:#}")
}

/// Advances the engine's epoch on a background thread.
struct Ticker {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl Ticker {
    fn start(engine: Engine) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let handle = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                std::thread::sleep(EPOCH_TICK);
                engine.increment_epoch();
            }
        });
        Self { stop, handle }
    }

    fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.handle.join();
    }
}
