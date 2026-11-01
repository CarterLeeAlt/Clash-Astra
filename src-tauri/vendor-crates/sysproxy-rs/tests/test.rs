#[cfg(test)]
mod tests {
    use sysproxy::Sysproxy;

    #[test]
    fn test_support() {
        assert!(Sysproxy::is_support());
    }

    #[test]
    fn test_get() {
        Sysproxy::get_system_proxy().unwrap();
    }
}
