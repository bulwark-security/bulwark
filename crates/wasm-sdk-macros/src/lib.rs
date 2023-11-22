use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{quote, quote_spanned};
use syn::{
    parse_macro_input, parse_quote, punctuated::Punctuated, spanned::Spanned, Attribute, Ident,
    ItemFn, ItemImpl, LitBool, LitStr, ReturnType, Signature, Visibility,
};
extern crate proc_macro;

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
/// - `on_init` - Not typically used. Called when the plugin is first loaded. If defined, overrides the
///   default macro behavior of calling
///   [`receive_request_body(true)`](https://docs.rs/bulwark-wasm-sdk/latest/bulwark_wasm_sdk/fn.receive_request_body.html)
///   or [`receive_response_body(true)`](https://docs.rs/bulwark-wasm-sdk/latest/bulwark_wasm_sdk/fn.receive_response_body.html)
///   when the corresponding handlers have been defined.
/// - `on_request` - This handler is called for every incoming request, before any decision-making will occur.
///   It is typically used to perform enrichment tasks with the
///   [`set_param_value`](https://docs.rs/bulwark-wasm-sdk/latest/bulwark_wasm_sdk/fn.set_param_value.html) function.
///   The request body will not yet be available when this handler is called.
/// - `on_request_decision` - This handler is called to make an initial decision.
/// - `on_request_body_decision` - This handler is called once the request body is available. The decision may be updated
///   with any new evidence found in the request body.
/// - `on_response_decision` - This handler is called once the interior service has received the request, processed it, and
///   returned a response, but prior to that response being sent onwards to the original exterior client. Notably, a `restricted`
///   outcome here does not cancel any actions or side-effects from the interior service that may have taken place already.
///   This handler is often used to process response status codes.
/// - `on_response_body_decision` - This handler is called once the response body is available. The decision may be updated
///   with any new evidence found in the response body.
/// - `on_decision_feedback` - This handler is called once a final verdict has been reached. The combined decision
///   of all plugins is available here, not just the decision of the currently executing plugin. This handler may be
///   used for any form of feedback loop, counter-based detections, or to train a model. Additionally, in the case of a
///   `restricted` outcome, this handler may be used to perform logouts or otherwise cancel or attempt to roll back undesired
///   side-effects that could have occurred prior to the verdict being rendered.
///
/// # Example
///
/// ```no_compile
/// use bulwark_wasm_sdk::*;
///
/// struct ExamplePlugin;
///
/// #[bulwark_plugin]
/// impl Handlers for ExamplePlugin {
///     fn on_request_decision() -> Result {
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
        if &trait_name != "Handlers" {
            return syn::Error::new(
                path.span(),
                format!(
                    "`bulwark_plugin` encountered unexpected trait `{}` for the impl",
                    trait_name
                ),
            )
            .to_compile_error()
            .into();
        }
    } else {
        return syn::Error::new(
            raw_impl.self_ty.span(),
            "`bulwark_plugin` requires an impl for the `Guest` trait",
        )
        .to_compile_error()
        .into();
    }

    let struct_type = &raw_impl.self_ty;
    let mut init_handler_found = false;
    let mut request_body_handler_found = false;
    let mut response_body_handler_found = false;

    let mut handlers = vec![
        "on_request",
        "on_request_decision",
        "on_request_body_decision",
        "on_response_decision",
        "on_response_body_decision",
        "on_decision_feedback",
    ];

    let mut new_items = Vec::with_capacity(raw_impl.items.len());
    for item in &raw_impl.items {
        if let syn::ImplItem::Fn(iifn) = item {
            match iifn.sig.ident.to_string().as_str() {
                "on_init" => init_handler_found = true,
                "on_request_body_decision" => request_body_handler_found = true,
                "on_response_body_decision" => response_body_handler_found = true,
                _ => {}
            }
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
            let handler_ident = Ident::new(handler_name, Span::call_site());
            quote! {
                #[handler]
                fn #handler_ident() -> Result {
                    Ok(())
                }
            }
        })
        .collect::<Vec<proc_macro2::TokenStream>>();

    let init_handler = if init_handler_found {
        // Empty token stream if an init handler was already defined, we'll generate nothing and use that instead
        quote! {}
    } else {
        let receive_request_body = LitBool::new(request_body_handler_found, Span::call_site());
        let receive_response_body = LitBool::new(response_body_handler_found, Span::call_site());
        quote! {
            #[handler]
            fn on_init() -> Result {
                receive_request_body(#receive_request_body);
                receive_response_body(#receive_response_body);
                Ok(())
            }
        }
    };

    let output = quote! {
        mod handlers {
            wit_bindgen::generate!({
                world: "bulwark:plugin/handlers",
                exports: {
                    world: crate::#struct_type
                }
            });
        }

        // type Handlers = crate::handlers::Guest;
        use crate::handlers::Guest as Handlers;
        impl Handlers for #struct_type {
            #init_handler
            #(#new_items)*
            #(#noop_handlers)*
        }
    };

    output.into()
}

/// The `handler` attribute makes the associated function into a Bulwark event handler.
///
/// The `handler` attribute is normally applied automatically by the `bulwark_plugin` macro and
/// need not be specified explicitly.
///
/// The associated function must take no parameters and return a `bulwark_wasm_sdk::Result`. It may only be
/// named one of the following:
/// - `on_init`
/// - `on_request`
/// - `on_request_decision`
/// - `on_request_body_decision`
/// - `on_response_decision`
/// - `on_response_body_decision`
/// - `on_decision_feedback`
#[doc(hidden)]
#[proc_macro_attribute]
pub fn handler(_: TokenStream, input: TokenStream) -> TokenStream {
    // Parse the input token stream as a free-standing function, or return an error.
    let raw_handler = parse_macro_input!(input as ItemFn);

    // Check that the function signature looks okay-ish. If we have the wrong number of arguments,
    // or no return type is specified , print a friendly spanned error with the expected signature.
    if !check_impl_signature(&raw_handler.sig) {
        return syn::Error::new(
            raw_handler.sig.span(),
            format!(
                "`handler` expects a function such as:

#[handler]
fn {}() -> Result {{
    ...
}}
",
                raw_handler.sig.ident
            ),
        )
        .to_compile_error()
        .into();
    }

    // Get the attributes and signature of the outer function. Then, update the
    // attributes and visibility of the inner function that we will inline.
    let (attrs, sig) = outer_handler_info(&raw_handler);
    let (name, inner_fn) = inner_fn_info(raw_handler);
    let name_str = LitStr::new(name.to_string().as_str(), Span::call_site());

    let output;

    match name.to_string().as_str() {
        "on_init"
        | "on_request"
        | "on_request_decision"
        | "on_request_body_decision"
        | "on_response_decision"
        | "on_response_body_decision"
        | "on_decision_feedback" => {
            output = quote_spanned! {inner_fn.span() =>
                #(#attrs)*
                #sig {
                    // Declares the inlined inner function, calls it, then performs very
                    // basic error handling on the result
                    #[inline(always)]
                    #inner_fn
                    let result = #name().map_err(|e| {
                        eprintln!("error in '{}' handler: {}", #name_str, e);
                        append_tags(["error"]);
                        // Absorbs the error, returning () to match desired signature
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
                
- `on_init`
- `on_request`
- `on_request_decision`
- `on_request_body_decision`
- `on_response_decision`
- `on_response_body_decision`
- `on_decision_feedback`
",
            )
            .to_compile_error()
            .into()
        }
    }

    output.into()
}

/// Check if the signature of the `#[handler]` function seems correct.
///
/// Unfortunately, precisely typecheck in a procedural macro attribute is not possible, because we
/// are dealing with [`TokenStream`]s. This checks that our signature takes one input, and has a
/// return type. Specific type errors are caught later, after the macro has been expanded.
///
/// This is used by the [`handler`] procedural macro attribute to help provide friendly errors
/// when given a function with the incorrect signature.
///
/// [`TokenStream`]: proc_macro/struct.TokenStream.html
fn check_impl_signature(sig: &Signature) -> bool {
    if sig.inputs.iter().len() != 0 {
        false // Return false if the signature takes no inputs, or more than one input.
    } else if let ReturnType::Default = sig.output {
        false // Return false if the signature's output type is empty.
    } else {
        true
    }
}

/// Returns a 2-tuple containing the attributes and signature of our outer `handler`.
///
/// The outer handler function will use the same attributes and visibility as our raw handler
/// function.
///
/// The signature of the outer function will be changed to have inputs and outputs of the form
/// `fn handler_name() -> Result<(), ()>`. The name of the outer handler function will be the same
/// as the inlined function.
fn outer_handler_info(inner_handler: &ItemFn) -> (Vec<Attribute>, Signature) {
    let attrs = inner_handler.attrs.clone();
    let name = inner_handler.sig.ident.to_string();
    let sig = {
        let mut sig = inner_handler.sig.clone();
        sig.ident = Ident::new(&name, Span::call_site());
        sig.inputs = Punctuated::new();
        sig.output = parse_quote!(-> ::std::result::Result<(), ()>);
        sig
    };

    (attrs, sig)
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
