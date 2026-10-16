pub fn greeting() -> String {
    "hello from ferrolint".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_names_the_app() {
        assert_eq!(greeting(), "hello from ferrolint");
    }
}
