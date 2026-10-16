//! The WASM plugin host: compile each plugin once, keep one instance per
//! worker thread, and run every check under an empty WASI environment
//! with a fuel budget and an epoch deadline per file.

use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use ferrolint_core::{RuleInfo, Severity};
use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Engine, Store, StoreLimits, StoreLimitsBuilder, Trap};
use wasmtime_wasi::{
    Deterministic, HostMonotonicClock, HostWallClock, WasiCtx, WasiCtxBuilder, WasiCtxView,
    WasiView,
};

use crate::error::{Error, Result};
use crate::semantic::Model;
use crate::wit::{self, Binding, Diagnostic, File, Node, Plugin, PluginPre, Scope};

/// Instructions one plugin may run on one file.
const FUEL_PER_FILE: u64 = 2_000_000_000;
/// How often the epoch advances.
const EPOCH_TICK: Duration = Duration::from_millis(10);
/// Epoch ticks one plugin may take on one file: ten seconds.
const EPOCHS_PER_FILE: u64 = 1_000;
/// Linear memory one plugin instance may grow to.
const MEMORY_LIMIT: usize = 512 << 20;

/// A file as the plugins see it: its node list and semantic model.
pub struct FileData {
    pub nodes: Vec<Node>,
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
        if (node as usize) < file.nodes.len() {
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
        Ok(file.model.visible_bindings(&file.nodes, node))
    }

    fn resolve(&mut self, node: u32) -> wasmtime::Result<Option<Binding>> {
        let file = self.check_node(node)?;
        Ok(file.model.resolve(&file.nodes, node))
    }
}

/// A plugin compiled and checked against the contract.
pub struct LoadedPlugin {
    pub name: String,
    pub rules: Vec<RuleInfo>,
    pre: PluginPre<State>,
    /// Keys this plugin's instances in each worker's cache.
    key: u64,
}

/// How a check call ended.
pub enum Outcome {
    Diagnostics(Vec<Diagnostic>),
    /// The plugin trapped or hit a cap; the reason says which.
    Aborted(String),
}

/// One live instance of a plugin, owned by one worker thread.
struct Live {
    store: Store<State>,
    plugin: Plugin,
}

thread_local! {
    static LIVE: RefCell<HashMap<u64, Live>> = RefCell::new(HashMap::new());
}

static NEXT_KEY: AtomicU64 = AtomicU64::new(0);

pub struct Host {
    engine: Engine,
    pub plugins: Vec<LoadedPlugin>,
    ticker: Option<Ticker>,
}

impl Host {
    /// Compile each plugin once, link it against the contract and read its
    /// rules. `plugins` holds each plugin's path and bytes, in run order.
    pub fn load(plugins: Vec<(PathBuf, Vec<u8>)>) -> Result<Self> {
        let mut config = wasmtime::Config::new();
        config
            .wasm_component_model(true)
            .consume_fuel(true)
            .epoch_interruption(true);
        let engine = Engine::new(&config)
            .map_err(|err| Error::plugin(format!("cannot start the WASM engine: {err}")))?;
        let mut linker = Linker::<State>::new(&engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
            .and_then(|()| {
                Plugin::add_to_linker::<State, HasSelf<State>>(&mut linker, |state| state)
            })
            .map_err(|err| Error::plugin(format!("cannot link the plugin contract: {err}")))?;

        let compiled: Vec<Result<LoadedPlugin>> = std::thread::scope(|scope| {
            let handles: Vec<_> = plugins
                .into_iter()
                .map(|(path, bytes)| {
                    let (engine, linker) = (&engine, &linker);
                    scope.spawn(move || load_one(engine, linker, &path, &bytes))
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
        let plugins = compiled.into_iter().collect::<Result<Vec<_>>>()?;
        let ticker = (!plugins.is_empty()).then(|| Ticker::start(engine.clone()));
        Ok(Self {
            engine,
            plugins,
            ticker,
        })
    }

    /// Run one plugin's check on one file on the current thread.
    pub fn check(&self, plugin: usize, file: &File, data: &Arc<FileData>) -> Outcome {
        let loaded = &self.plugins[plugin];
        LIVE.with(|live| {
            let mut live = live.borrow_mut();
            let instance = match live.entry(loaded.key) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => match instantiate(&self.engine, &loaded.pre) {
                    Ok(instance) => entry.insert(instance),
                    Err(err) => return Outcome::Aborted(describe(&err)),
                },
            };
            instance.store.data_mut().file = Some(Arc::clone(data));
            let result = arm(&mut instance.store).and_then(|()| {
                instance.plugin.ferrolint_plugin_checker().call_check(
                    &mut instance.store,
                    file,
                    &data.nodes,
                )
            });
            instance.store.data_mut().file = None;
            match result {
                Ok(diagnostics) => Outcome::Diagnostics(diagnostics),
                Err(err) => {
                    // A trapped instance cannot be entered again.
                    live.remove(&loaded.key);
                    Outcome::Aborted(describe(&err))
                }
            }
        })
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        if let Some(ticker) = self.ticker.take() {
            ticker.stop();
        }
    }
}

fn load_one(
    engine: &Engine,
    linker: &Linker<State>,
    path: &Path,
    bytes: &[u8],
) -> Result<LoadedPlugin> {
    let fail = |what: &str, err: &dyn std::fmt::Display| {
        Error::plugin(format!("{}: {what}: {err:#}", path.display()))
    };
    let component =
        Component::new(engine, bytes).map_err(|err| fail("not a WASM component", &err))?;
    let pre = linker
        .instantiate_pre(&component)
        .map_err(|err| fail("imports outside the plugin contract", &err))?;
    let pre =
        PluginPre::new(pre).map_err(|err| fail("does not implement the plugin contract", &err))?;
    let mut instance = instantiate(engine, &pre).map_err(|err| fail("cannot instantiate", &err))?;
    let info = arm(&mut instance.store)
        .and_then(|()| {
            instance
                .plugin
                .ferrolint_plugin_checker()
                .call_info(&mut instance.store)
        })
        .map_err(|err| fail("info() failed", &describe(&err)))?;
    let rules = rules(path, &info)?;
    Ok(LoadedPlugin {
        name: info.name,
        rules,
        pre,
        key: NEXT_KEY.fetch_add(1, Ordering::Relaxed),
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
