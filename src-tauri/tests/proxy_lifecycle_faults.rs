mod application {
    pub(crate) mod health_protection {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub(crate) struct HealthProtectionScope;
    }

    pub(crate) mod request_finalization {
        pub(crate) mod failure {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(crate) enum RetryDisposition {
                TryNextKey,
                StopRequest,
            }
        }
    }

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
