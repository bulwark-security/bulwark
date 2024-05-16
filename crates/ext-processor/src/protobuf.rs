// pub use pbjson_types;
// pub use prost;
// pub use prost_wkt_types;

pub mod envoy {
    pub mod config {
        pub mod core {
            pub mod v3 {
                // tonic::include_proto!("xds");
                include!(concat!(env!("OUT_DIR"), "/envoy.config.core.v3.rs"));
                // include!(concat!(env!("OUT_DIR"), "/envoy.config.core.v3.serde.rs"));
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
                        // include!(concat!(
                        //     env!("OUT_DIR"),
                        //     "/envoy.extensions.filters.http.ext_authz.v3.serde.rs"
                        // ));
                    }
                }
                pub mod ext_proc {
                    pub mod v3 {
                        include!(concat!(
                            env!("OUT_DIR"),
                            "/envoy.extensions.filters.http.ext_proc.v3.rs"
                        ));
                        // include!(concat!(
                        //     env!("OUT_DIR"),
                        //     "/envoy.extensions.filters.http.ext_proc.v3.serde.rs"
                        // ));
                    }
                }
            }
        }
    }
    pub mod r#type {
        pub mod matcher {
            pub mod v3 {
                include!(concat!(env!("OUT_DIR"), "/envoy.r#type.matcher.v3.rs"));
                // include!(concat!(
                //     env!("OUT_DIR"),
                //     "/envoy.r#type.matcher.v3.serde.rs"
                // ));
            }
        }
        pub mod v3 {
            include!(concat!(env!("OUT_DIR"), "/envoy.r#type.v3.rs"));
            // include!(concat!(env!("OUT_DIR"), "/envoy.r#type.v3.serde.rs"));
        }
    }
    pub mod service {
        pub mod auth {
            pub mod v3 {
                // tonic::include_proto!("google");
                include!(concat!(env!("OUT_DIR"), "/envoy.service.auth.v3.rs"));
                // include!(concat!(env!("OUT_DIR"), "/envoy.service.auth.v3.serde.rs"));
            }
        }
        pub mod ext_proc {
            pub mod v3 {
                include!(concat!(env!("OUT_DIR"), "/envoy.service.ext_proc.v3.rs"));
                // include!(concat!(env!("OUT_DIR"), "/envoy.service.ext_proc.v3.serde.rs"));
            }
        }
    }
}
pub mod google {
    pub mod rpc {
        include!(concat!(env!("OUT_DIR"), "/google.rpc.rs"));
        // include!(concat!(env!("OUT_DIR"), "/google.rpc.serde.rs"));
    }
}
pub mod udpa {
    pub mod annotations {
        include!(concat!(env!("OUT_DIR"), "/udpa.annotations.rs"));
        // include!(concat!(env!("OUT_DIR"), "/udpa.annotations.serde.rs"));
    }
}
pub mod validate {
    include!(concat!(env!("OUT_DIR"), "/validate.rs"));
    // include!(concat!(env!("OUT_DIR"), "/validate.serde.rs"));
}
pub mod xds {
    pub mod core {
        pub mod v3 {
            include!(concat!(env!("OUT_DIR"), "/xds.core.v3.rs"));
            // include!(concat!(env!("OUT_DIR"), "/xds.core.v3.serde.rs"));
        }
    }
}
