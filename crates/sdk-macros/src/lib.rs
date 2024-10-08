use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::{parse_macro_input, parse_quote, spanned::Spanned, Ident, ItemFn, ItemImpl, Visibility};
extern crate proc_macro;

const WIT_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/wit");

/// The `bulwark_plugin` attribute generates default implementations for all handler traits in a module
/// and produces friendly errors for common mistakes.
///
/// All trait functions for `Handlers` are optional when used in conjunction with this macro. A no-op
/// implementation will be automatically generated if a handler function has not been defined. Handler
/// functions are called in sequence, in the order below. All `*_decision` handlers render an updated
/// decision. In the case of a `restricted` outcome, no further processing will occur. Otherwise,
/// processing will continue to the next handler.
///
/// # Trait Functions
/// - `handle_init` - Rarely used. Called when the plugin is first loaded.
/// - `handle_request_enrichment` - This handler is called for every incoming request, before any decision-making will occur.
///   It is typically used to perform enrichment tasks.
/// - `handle_request_decision` - This handler is called to make an initial decision.
/// - `handle_response_decision` - This handler is called once the interior service has received the request, processed it, and
///   returned a response, but prior to that response being sent onwards to the original exterior client. Notably, a `restricted`
///   outcome here does not cancel any actions or side-effects from the interior service that may have taken place already.
///   This handler is often used to process response status codes.
/// - `handle_decision_feedback` - This handler is called once a final verdict has been reached. The combined decision
///   of all plugins is available here, not just the decision of the currently executing plugin. This handler may be
///   used for any form of feedback loop, counter-based detections, or to train a model. Additionally, in the case of a
///   `restricted` outcome, this handler may be used to perform logouts or otherwise cancel or attempt to roll back undesired
///   side-effects that could have occurred prior to the verdict being rendered.
///
/// # Example
///
#[cfg_attr(doctest, doc = " ````no_test")] // highlight, but don't run the test (rust/issues/63193)
/// ```rust
/// use bulwark_sdk::*;
/// use std::collections::HashMap;
///
/// struct ExamplePlugin;
///
/// #[bulwark_plugin]
/// impl HttpHandlers for ExamplePlugin {
///     fn handle_request_decision(
///         req: http::Request,
///         labels: HashMap<String, String>,
///     ) -> Result<HandlerOutput, Error> {
///         println!("hello world");
///         // implement detection logic here
///         Ok(())
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn bulwark_plugin(_: TokenStream, input: TokenStream) -> TokenStream {
    // Parse the input token stream as an impl, or return an error.
    let raw_impl = parse_macro_input!(input as ItemImpl);

    // The trait must be specified by the developer even though there's only one valid value.
    // If we inject it, that leads to a very surprising result when developers try to define helper functions
    // in the same struct impl and can't because it's really a trait impl.
    if let Some((_, path, _)) = raw_impl.trait_ {
        let trait_name = path.get_ident().map_or(String::new(), |id| id.to_string());
        if &trait_name != "HttpHandlers" {
            return syn::Error::new(
                path.span(),
                format!(
                    "`bulwark_plugin` expected `HttpHandlers` trait, encountered unexpected trait `{}` for the impl",
                    trait_name
                ),
            )
            .to_compile_error()
            .into();
        }
    } else {
        return syn::Error::new(
            raw_impl.self_ty.span(),
            "`bulwark_plugin` requires an impl for the guest `HttpHandlers` trait",
        )
        .to_compile_error()
        .into();
    }

    let struct_type = &raw_impl.self_ty;

    let mut handlers = vec![
        "handle_init",
        "handle_request_enrichment",
        "handle_request_decision",
        "handle_response_decision",
        "handle_decision_feedback",
    ];

    let mut new_items = Vec::with_capacity(raw_impl.items.len());
    for item in &raw_impl.items {
        if let syn::ImplItem::Fn(iifn) = item {
            let initial_len = handlers.len();
            // Find and record the implemented handlers, removing any we find from the list above.
            handlers.retain(|h| *h != iifn.sig.ident.to_string().as_str());
            // Verify that any functions with a handler name we find have set the `handler` attribute.
            let mut use_original_item = true;
            if handlers.len() < initial_len {
                let mut handler_attr_found = false;
                for attr in &iifn.attrs {
                    if let Some(ident) = attr.meta.path().get_ident() {
                        if ident.to_string().as_str() == "handler" {
                            handler_attr_found = true;
                            break;
                        }
                    }
                }
                if !handler_attr_found {
                    use_original_item = false;
                    let mut new_iifn = iifn.clone();
                    new_iifn.attrs.push(parse_quote! {
                        #[handler]
                    });
                    new_items.push(syn::ImplItem::Fn(new_iifn));
                }
            }
            if use_original_item {
                new_items.push(item.clone());
            }
        } else {
            new_items.push(item.clone());
        }
    }

    // Define the missing handlers with no-op defaults
    let noop_handlers = handlers
        .iter()
        .map(|handler_name| {
            match *handler_name {
                "handle_init" => {
                    // handle-init: func() -> result<_, error>;
                    quote! {
                        #[handler]
                        fn handle_init() -> Result<(), ::bulwark_sdk::Error> {
                            Ok(())
                        }
                    }
                }
                "handle_request_enrichment" => {
                    quote! {
                        #[handler]
                        fn handle_request_enrichment(
                            _: ::bulwark_sdk::http::Request,
                            _: ::std::collections::HashMap<String, String>
                        ) -> Result<::std::collections::HashMap<String, String>, ::bulwark_sdk::Error> {
                            Ok(::std::collections::HashMap::new())
                        }
                    }
                }
                "handle_request_decision" => {
                    quote! {
                        #[handler]
                        fn handle_request_decision(
                            _: ::bulwark_sdk::http::Request,
                            _: ::std::collections::HashMap<String, String>
                        ) -> Result<::bulwark_sdk::HandlerOutput, ::bulwark_sdk::Error> {
                            Ok(::bulwark_sdk::HandlerOutput {
                                labels: ::std::collections::HashMap::new(),
                                decision: ::bulwark_sdk::Decision::default(),
                                tags: vec![],
                            })
                        }
                    }
                }
                "handle_response_decision" => {
                    quote! {
                        #[handler]
                        fn handle_response_decision(
                            _: ::bulwark_sdk::http::Request,
                            _: ::bulwark_sdk::http::Response,
                            _: ::std::collections::HashMap<String, String>
                        ) -> Result<::bulwark_sdk::HandlerOutput, ::bulwark_sdk::Error> {
                            Ok(::bulwark_sdk::HandlerOutput {
                                labels: ::std::collections::HashMap::new(),
                                decision: ::bulwark_sdk::Decision::default(),
                                tags: vec![],
                            })
                        }
                    }
                }
                "handle_decision_feedback" => {
                    quote! {
                        #[handler]
                        fn handle_decision_feedback(
                            _: ::bulwark_sdk::http::Request,
                            _: ::bulwark_sdk::http::Response,
                            _: ::std::collections::HashMap<String, String>,
                            _: ::bulwark_sdk::Verdict,
                        ) -> Result<(), ::bulwark_sdk::Error> {
                            Ok(())
                        }
                    }
                }
                _ => {
                    syn::Error::new(
                        raw_impl.self_ty.span(),
                        "Could not generate no-op handler for the guest `HttpHandlers` trait",
                    )
                    .to_compile_error()
                }
            }
        })
        .collect::<Vec<proc_macro2::TokenStream>>();

    let output = quote! {
        mod handlers {
            use super::#struct_type;

            ::bulwark_sdk::wit_bindgen::generate!({
                world: "bulwark:plugin/http-detection",
                path: #WIT_PATH,
                runtime_path: "::bulwark_sdk::wit_bindgen::rt",
            });
        }

        impl From<crate::handlers::bulwark::plugin::types::Decision> for ::bulwark_sdk::Decision {
            fn from(decision: crate::handlers::bulwark::plugin::types::Decision) -> Self {
                Self {
                    accept: decision.accepted,
                    restrict: decision.restricted,
                    unknown: decision.unknown,
                }
            }
        }

        impl From<::bulwark_sdk::Decision> for crate::handlers::bulwark::plugin::types::Decision {
            fn from(decision: ::bulwark_sdk::Decision) -> Self {
                Self {
                    accepted: decision.accept,
                    restricted: decision.restrict,
                    unknown: decision.unknown,
                }
            }
        }

        impl From<crate::handlers::exports::bulwark::plugin::http_handlers::HandlerOutput> for ::bulwark_sdk::HandlerOutput {
            fn from(handler_output: crate::handlers::exports::bulwark::plugin::http_handlers::HandlerOutput) -> Self {
                Self {
                    labels: handler_output.labels.iter().cloned().collect(),
                    decision: handler_output.decision.into(),
                    tags: handler_output.tags.clone(),
                }
            }
        }

        impl From<bulwark_sdk::HandlerOutput> for crate::handlers::exports::bulwark::plugin::http_handlers::HandlerOutput {
            fn from(handler_output: ::bulwark_sdk::HandlerOutput) -> Self {
                Self {
                    labels: handler_output.labels.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                    decision: handler_output.decision.into(),
                    tags: handler_output.tags.clone(),
                }
            }
        }

        impl From<crate::handlers::bulwark::plugin::types::Outcome> for ::bulwark_sdk::Outcome {
            fn from(outcome: crate::handlers::bulwark::plugin::types::Outcome) -> Self {
                match outcome {
                    crate::handlers::bulwark::plugin::types::Outcome::Trusted => Self::Trusted,
                    crate::handlers::bulwark::plugin::types::Outcome::Accepted => Self::Accepted,
                    crate::handlers::bulwark::plugin::types::Outcome::Suspected => Self::Suspected,
                    crate::handlers::bulwark::plugin::types::Outcome::Restricted => Self::Restricted,
                }
            }
        }

        impl From<crate::handlers::bulwark::plugin::types::Verdict> for ::bulwark_sdk::Verdict {
            fn from(verdict: crate::handlers::bulwark::plugin::types::Verdict) -> Self {
                Self {
                    decision: verdict.decision.into(),
                    outcome: verdict.outcome.into(),
                    count: verdict.count,
                    tags: verdict.tags.clone(),
                }
            }
        }

        impl TryFrom<crate::handlers::wasi::http::types::IncomingRequest> for ::bulwark_sdk::http::Request {
            type Error = crate::handlers::exports::bulwark::plugin::http_handlers::Error;

            fn try_from(request: crate::handlers::wasi::http::types::IncomingRequest) -> Result<Self, Self::Error> {
                const MAX_SIZE: u64 = 1048576;
                let mut builder = ::bulwark_sdk::http::request::Builder::new();
                // Builder doesn't support scheme or authority as separate functions,
                // so we need to manually construct the URI.
                let mut uri = ::bulwark_sdk::http::uri::Builder::new();
                if let Some(scheme) = request.scheme() {
                    let other;
                    uri = uri.scheme(match scheme {
                        crate::handlers::wasi::http::types::Scheme::Http => "http",
                        crate::handlers::wasi::http::types::Scheme::Https => "https",
                        crate::handlers::wasi::http::types::Scheme::Other(o) => {
                            other = o;
                            other.as_str()
                        },
                    });
                }
                if let Some(authority) = request.authority() {
                    uri = uri.authority(authority);
                }
                let other;
                let method = match request.method() {
                    crate::handlers::wasi::http::types::Method::Get => "GET",
                    crate::handlers::wasi::http::types::Method::Head => "HEAD",
                    crate::handlers::wasi::http::types::Method::Post => "POST",
                    crate::handlers::wasi::http::types::Method::Put => "PUT",
                    crate::handlers::wasi::http::types::Method::Delete => "DELETE",
                    crate::handlers::wasi::http::types::Method::Connect => "CONNECT",
                    crate::handlers::wasi::http::types::Method::Options => "OPTIONS",
                    crate::handlers::wasi::http::types::Method::Trace => "TRACE",
                    crate::handlers::wasi::http::types::Method::Patch => "PATCH",
                    crate::handlers::wasi::http::types::Method::Other(o) => {
                        other = o;
                        other.as_str()
                    },
                };
                builder = builder.method(method);
                if let Some(request_uri) = request.path_with_query() {
                    uri = uri.path_and_query(request_uri);
                }
                // This should always be a valid URI, panics if it's not.
                builder = builder.uri(uri.build().expect("invalid uri"));
                let mut end_of_stream = true;
                let headers = request.headers().entries();
                for (name, value) in headers {
                    if name.eq_ignore_ascii_case("content-length") {
                        if let Ok(value) = std::str::from_utf8(&value) {
                            if let Ok(value) = value.parse::<u64>() {
                                if value > 0 {
                                    end_of_stream = false;
                                }
                            }
                        }
                    }
                    if name.eq_ignore_ascii_case("transfer-encoding") {
                        end_of_stream = false;
                    }
                    builder = builder.header(name, value);
                }

                let mut buffer = Vec::new();
                if !end_of_stream {
                    // A body should be available, extract it.
                    let body = request.consume().map_err(|_| {
                        crate::handlers::exports::bulwark::plugin::http_handlers::Error::Other("body cannot be consumed".to_string())
                    })?;
                    let mut stream = body.stream().map_err(|_| {
                        crate::handlers::exports::bulwark::plugin::http_handlers::Error::Other("could not get body stream".to_string())
                    })?;
                    buffer.extend(stream.read(MAX_SIZE).map_err(|e| {
                        crate::handlers::exports::bulwark::plugin::http_handlers::Error::Other(e.to_string())
                    })?);
                }

                // TODO: Add support for trailers?

                builder.body(bulwark_sdk::Bytes::from(buffer)).map_err(|e| {
                    crate::handlers::exports::bulwark::plugin::http_handlers::Error::Other(e.to_string())
                })
            }
        }

        impl TryFrom<crate::handlers::wasi::http::types::IncomingResponse> for ::bulwark_sdk::http::Response {
            type Error = crate::handlers::exports::bulwark::plugin::http_handlers::Error;

            fn try_from(response: crate::handlers::wasi::http::types::IncomingResponse) -> Result<Self, Self::Error> {
                const MAX_SIZE: u64 = 1048576;
                let mut builder = ::bulwark_sdk::http::response::Builder::new();
                // We have no way to know the HTTP version here, so leave it as default.
                builder = builder.status(response.status());

                let mut end_of_stream = true;
                let headers = response.headers().entries();
                for (name, value) in headers {
                    if name.eq_ignore_ascii_case("content-length") {
                        if let Ok(value) = std::str::from_utf8(&value) {
                            if let Ok(value) = value.parse::<u64>() {
                                if value > 0 {
                                    end_of_stream = false;
                                }
                            }
                        }
                    }
                    if name.eq_ignore_ascii_case("transfer-encoding") {
                        end_of_stream = false;
                    }
                    builder = builder.header(name, value);
                }

                let mut buffer = Vec::new();
                if !end_of_stream {
                    // A body should be available, extract it.
                    let body = response.consume().map_err(|_| {
                        crate::handlers::exports::bulwark::plugin::http_handlers::Error::Other("body cannot be consumed".to_string())
                    })?;
                    let mut stream = body.stream().map_err(|_| {
                        crate::handlers::exports::bulwark::plugin::http_handlers::Error::Other("could not get body stream".to_string())
                    })?;
                    buffer.extend(stream.read(MAX_SIZE).map_err(|e| {
                        crate::handlers::exports::bulwark::plugin::http_handlers::Error::Other(e.to_string())
                    })?);
                }

                // TODO: Add support for trailers?

                builder.body(bulwark_sdk::Bytes::from(buffer)).map_err(|e| {
                    crate::handlers::exports::bulwark::plugin::http_handlers::Error::Other(e.to_string())
                })
            }
        }

        use crate::handlers::exports::bulwark::plugin::http_handlers::Guest as HttpHandlers;
        impl HttpHandlers for #struct_type {
            #(#new_items)*
            #(#noop_handlers)*
        }

        handlers::export!(#struct_type with_types_in handlers);
    };

    output.into()
}

/// The `handler` attribute makes the associated function into a Bulwark event handler.
///
/// The `handler` attribute is normally applied automatically by the `bulwark_plugin` macro and
/// need not be specified explicitly.
///
/// The associated function must have the correct signature and may only be named one of the following:
/// - `handle_init`
/// - `handle_request_enrichment`
/// - `handle_request_decision`
/// - `handle_response_decision`
/// - `handle_decision_feedback`
#[doc(hidden)]
#[proc_macro_attribute]
pub fn handler(_: TokenStream, input: TokenStream) -> TokenStream {
    // Parse the input token stream as a free-standing function, or return an error.
    let raw_handler = parse_macro_input!(input as ItemFn);

    // Get the attributes and signature of the outer function. Then, update the
    // attributes and visibility of the inner function that we will inline.
    let attrs = raw_handler.attrs.clone();
    let (name, inner_fn) = inner_fn_info(raw_handler);

    let output;

    // The weird variable naming for these functions is to avoid accidentally shadowing any parameter names
    // used by the inner function, which can result in difficult-to-read compiler errors if a plugin typos
    // a variable name.
    match name.to_string().as_str() {
        "handle_init" => {
            output = quote_spanned! {inner_fn.span() =>
                #(#attrs)*
                fn handle_init() -> Result<(), handlers::exports::bulwark::plugin::http_handlers::Error> {
                    // Declares the inlined inner function, calls it, then performs very
                    // basic error handling on the result
                    #[inline(always)]
                    #inner_fn
                    let result = #name().map_err(|e| {
                        handlers::exports::bulwark::plugin::http_handlers::Error::Other(e.to_string())
                    });
                    #[allow(unused_must_use)]
                    {
                        // Apparently we can exit the guest environment before IO is flushed,
                        // causing it to never be captured? This ensures IO is flushed and captured.
                        use std::io::Write;
                        std::io::stdout().flush();
                        std::io::stderr().flush();
                    }
                    result
                }
            }
        }
        "handle_request_enrichment" => {
            output = quote_spanned! {inner_fn.span() =>
                #(#attrs)*
                fn handle_request_enrichment(
                    in_reqst: handlers::wasi::http::types::IncomingRequest,
                    lbls: wit_bindgen::rt::vec::Vec<handlers::bulwark::plugin::types::Label>,
                ) -> Result<
                    wit_bindgen::rt::vec::Vec<handlers::bulwark::plugin::types::Label>,
                    handlers::exports::bulwark::plugin::http_handlers::Error,
                > {
                    // Declares the inlined inner function, calls it, then performs very
                    // basic error handling on the result
                    #[inline(always)]
                    #inner_fn
                    let result = #name(in_reqst.try_into()?, lbls.iter().cloned().collect()).map(|t| {
                        t.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                    }).map_err(|e| {
                        handlers::exports::bulwark::plugin::http_handlers::Error::Other(e.to_string())
                    });
                    #[allow(unused_must_use)]
                    {
                        // Apparently we can exit the guest environment before IO is flushed,
                        // causing it to never be captured? This ensures IO is flushed and captured.
                        use std::io::Write;
                        std::io::stdout().flush();
                        std::io::stderr().flush();
                    }
                    result
                }
            }
        }
        "handle_request_decision" => {
            output = quote_spanned! {inner_fn.span() =>
                #(#attrs)*
                fn handle_request_decision(
                    in_reqst: handlers::wasi::http::types::IncomingRequest,
                    lbls: wit_bindgen::rt::vec::Vec<handlers::bulwark::plugin::types::Label>,
                ) -> Result<
                    handlers::exports::bulwark::plugin::http_handlers::HandlerOutput,
                    handlers::exports::bulwark::plugin::http_handlers::Error,
                > {
                    // Declares the inlined inner function, calls it, then translates the result
                    #[inline(always)]
                    #inner_fn
                    let result = #name(in_reqst.try_into()?, lbls.iter().cloned().collect()).map(|t| {
                        t.into()
                    }).map_err(|e| {
                        handlers::exports::bulwark::plugin::http_handlers::Error::Other(e.to_string())
                    });
                    #[allow(unused_must_use)]
                    {
                        // Apparently we can exit the guest environment before IO is flushed,
                        // causing it to never be captured? This ensures IO is flushed and captured.
                        use std::io::Write;
                        std::io::stdout().flush();
                        std::io::stderr().flush();
                    }
                    result
                }
            }
        }
        "handle_response_decision" => {
            output = quote_spanned! {inner_fn.span() =>
                #(#attrs)*
                fn handle_response_decision(
                    in_reqst: handlers::wasi::http::types::IncomingRequest,
                    in_respn: handlers::wasi::http::types::IncomingResponse,
                    lbls: wit_bindgen::rt::vec::Vec<handlers::bulwark::plugin::types::Label>,
                ) -> Result<
                    handlers::exports::bulwark::plugin::http_handlers::HandlerOutput,
                    handlers::exports::bulwark::plugin::http_handlers::Error,
                > {
                    // Declares the inlined inner function, calls it, then performs very
                    // basic error handling on the result
                    #[inline(always)]
                    #inner_fn
                    let result = #name(in_reqst.try_into()?, in_respn.try_into()?, lbls.iter().cloned().collect()).map(|t| {
                        t.into()
                    }).map_err(|e| {
                        handlers::exports::bulwark::plugin::http_handlers::Error::Other(e.to_string())
                    });
                    #[allow(unused_must_use)]
                    {
                        // Apparently we can exit the guest environment before IO is flushed,
                        // causing it to never be captured? This ensures IO is flushed and captured.
                        use std::io::Write;
                        std::io::stdout().flush();
                        std::io::stderr().flush();
                    }
                    result
                }
            }
        }
        "handle_decision_feedback" => {
            output = quote_spanned! {inner_fn.span() =>
                #(#attrs)*
                fn handle_decision_feedback(
                    in_reqst: handlers::wasi::http::types::IncomingRequest,
                    in_respn: handlers::wasi::http::types::IncomingResponse,
                    lbls: wit_bindgen::rt::vec::Vec<handlers::bulwark::plugin::types::Label>,
                    vrdct: handlers::bulwark::plugin::types::Verdict,
                ) -> Result<(), handlers::exports::bulwark::plugin::http_handlers::Error> {
                    // Declares the inlined inner function, calls it, then performs very
                    // basic error handling on the result
                    #[inline(always)]
                    #inner_fn
                    let result = #name(in_reqst.try_into()?, in_respn.try_into()?, lbls.iter().cloned().collect(), vrdct.into()).map_err(|e| {
                        handlers::exports::bulwark::plugin::http_handlers::Error::Other(e.to_string())
                    });
                    #[allow(unused_must_use)]
                    {
                        // Apparently we can exit the guest environment before IO is flushed,
                        // causing it to never be captured? This ensures IO is flushed and captured.
                        use std::io::Write;
                        std::io::stdout().flush();
                        std::io::stderr().flush();
                    }
                    result
                }
            }
        }
        _ => {
            return syn::Error::new(
                inner_fn.sig.span(),
                "`handler` expects a function named one of:

- `handle_init`
- `handle_request_enrichment`
- `handle_request_decision`
- `handle_response_decision`
- `handle_decision_feedback`
",
            )
            .to_compile_error()
            .into()
        }
    }

    output.into()
}

/// Prepare our inner function to be inlined into our outer handler function.
///
/// This changes its visibility to [`Inherited`], and removes [`no_mangle`] from the attributes of
/// the inner function if it is there.
///
/// This function returns a 2-tuple of the inner function's identifier and the function itself.
///
/// [`Inherited`]: syn/enum.Visibility.html#variant.Inherited
/// [`no_mangle`]: https://doc.rust-lang.org/reference/abi.html#the-no_mangle-attribute
fn inner_fn_info(mut inner_handler: ItemFn) -> (Ident, ItemFn) {
    let name = inner_handler.sig.ident.clone();
    inner_handler.vis = Visibility::Inherited;
    inner_handler
        .attrs
        .retain(|attr| !attr.path().is_ident("no_mangle"));
    (name, inner_handler)
}
