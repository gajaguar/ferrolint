/// The plugin files among `names`, in name order.
pub fn plugin_files(mut names: Vec<String>) -> Vec<String> {
    names.retain(|name| {
        name.strip_prefix("ferrolint_plugin_")
            .is_some_and(|rest| rest.strip_suffix(".wasm").is_some())
    });
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::plugin_files;

    #[test]
    fn keeps_plugins_only_in_name_order() {
        let names = [
            "ferrolint_test_x.wasm",
            "ferrolint_plugin_b.wasm",
            "ferrolint_plugin_a.wasm",
            "ferrolint_plugin_c.d",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            plugin_files(names),
            ["ferrolint_plugin_a.wasm", "ferrolint_plugin_b.wasm"]
        );
    }
}
