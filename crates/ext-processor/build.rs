// Copyright 2021 The rust-control-plane Authors. All rights reserved.
// Use of this source code is governed by the Apache License,
// Version 2.0, that can be found in the protobuf/LICENSE file.

use core::panic;
use std::env;
use std::io::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    let descriptor_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("proto_descriptor.bin");
    println!("# {}", descriptor_path.display());
    let pwd = std::env::current_dir().unwrap();
    println!("# {}", pwd.display());
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .file_descriptor_set_path(descriptor_path.clone())
        .compile(
            &[
                "protobuf/data-plane-api/envoy/config/common/mutation_rules/v3/mutation_rules.proto",
                "protobuf/data-plane-api/envoy/config/core/v3/address.proto",
                "protobuf/data-plane-api/envoy/config/core/v3/base.proto",
                "protobuf/data-plane-api/envoy/config/core/v3/socket_option.proto",
                "protobuf/data-plane-api/envoy/extensions/filters/http/ext_authz/v3/ext_authz.proto",
                "protobuf/data-plane-api/envoy/extensions/filters/http/ext_proc/v3/ext_proc.proto",
                "protobuf/data-plane-api/envoy/extensions/filters/http/ext_proc/v3/processing_mode.proto",
                "protobuf/data-plane-api/envoy/extensions/filters/network/ext_authz/v3/ext_authz.proto",
                "protobuf/data-plane-api/envoy/service/auth/v3/attribute_context.proto",
                "protobuf/data-plane-api/envoy/service/auth/v3/external_auth.proto",
                "protobuf/data-plane-api/envoy/service/ext_proc/v3/external_processor.proto",
                "protobuf/data-plane-api/envoy/type/http/v3/path_transformation.proto",
                "protobuf/data-plane-api/envoy/type/matcher/v3/http_inputs.proto",
                "protobuf/data-plane-api/envoy/type/matcher/v3/metadata.proto",
                "protobuf/data-plane-api/envoy/type/matcher/v3/node.proto",
                "protobuf/data-plane-api/envoy/type/matcher/v3/number.proto",
                "protobuf/data-plane-api/envoy/type/matcher/v3/path.proto",
                "protobuf/data-plane-api/envoy/type/matcher/v3/regex.proto",
                "protobuf/data-plane-api/envoy/type/matcher/v3/string.proto",
                "protobuf/data-plane-api/envoy/type/matcher/v3/struct.proto",
                "protobuf/data-plane-api/envoy/type/matcher/v3/value.proto",
                "protobuf/data-plane-api/envoy/type/metadata/v3/metadata.proto",
                "protobuf/data-plane-api/envoy/type/tracing/v3/custom_tag.proto",
                "protobuf/data-plane-api/envoy/type/v3/hash_policy.proto",
                "protobuf/data-plane-api/envoy/type/v3/http.proto",
                "protobuf/data-plane-api/envoy/type/v3/http_status.proto",
                "protobuf/data-plane-api/envoy/type/v3/percent.proto",
                "protobuf/data-plane-api/envoy/type/v3/range.proto",
                "protobuf/data-plane-api/envoy/type/v3/ratelimit_unit.proto",
                "protobuf/data-plane-api/envoy/type/v3/semantic_version.proto",
                "protobuf/data-plane-api/envoy/type/v3/token_bucket.proto",
            ],
            &[
                "protobuf/data-plane-api/",
                "protobuf/xds/",
                "protobuf/protoc-gen-validate/",
                "protobuf/googleapis/",
            ],
        )?;

    Ok(())
}
