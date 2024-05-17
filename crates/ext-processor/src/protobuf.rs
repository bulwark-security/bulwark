pub mod envoy {
    pub mod config {
        pub mod common {
            pub mod mutation_rules {
                pub mod v3 {
                    include!(concat!(env!("OUT_DIR"), "/envoy.config.common.mutation_rules.v3.rs"));
                }
            }
        }
        pub mod core {
            #[allow(clippy::large_enum_variant)]
            pub mod v3 {
                include!(concat!(env!("OUT_DIR"), "/envoy.config.core.v3.rs"));
            }
        }
    }
    pub mod extensions {
        pub mod filters {
            pub mod http {
                pub mod ext_authz {
                    pub mod v3 {
                        include!(concat!(
                            env!("OUT_DIR"),
                            "/envoy.extensions.filters.http.ext_authz.v3.rs"
                        ));
                    }
                }
                pub mod ext_proc {
                    pub mod v3 {
                        include!(concat!(
                            env!("OUT_DIR"),
                            "/envoy.extensions.filters.http.ext_proc.v3.rs"
                        ));
                    }
                }
            }
        }
    }
    pub mod r#type {
        pub mod matcher {
            pub mod v3 {
                include!(concat!(env!("OUT_DIR"), "/envoy.r#type.matcher.v3.rs"));
            }
        }
        pub mod v3 {
            include!(concat!(env!("OUT_DIR"), "/envoy.r#type.v3.rs"));
        }
    }
    pub mod service {
        pub mod auth {
            pub mod v3 {
                include!(concat!(env!("OUT_DIR"), "/envoy.service.auth.v3.rs"));
            }
        }
        pub mod ext_proc {
            pub mod v3 {
                include!(concat!(env!("OUT_DIR"), "/envoy.service.ext_proc.v3.rs"));
            }
        }
    }
}
pub mod google {
    pub mod rpc {
        include!(concat!(env!("OUT_DIR"), "/google.rpc.rs"));
    }
}
pub mod xds {
    pub mod core {
        pub mod v3 {
            include!(concat!(env!("OUT_DIR"), "/xds.core.v3.rs"));
        }
    }
}
