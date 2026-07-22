mod application {
    #[path = "../../src/application/request_lifecycle/mod.rs"]
    pub(crate) mod request_lifecycle;
}

mod services {
    pub(crate) mod proxy {
        #[path = "../../../src/services/proxy/lifecycle/mod.rs"]
        pub(crate) mod lifecycle;

        #[path = "../../../src/services/proxy/lifecycle_fault_tests.rs"]
        mod lifecycle_fault_tests;
    }
}
