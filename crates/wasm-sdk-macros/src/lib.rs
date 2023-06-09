use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{quote, quote_spanned};
use syn::{
    parse_macro_input, parse_quote, punctuated::Punctuated, spanned::Spanned, Attribute, Ident,
    ItemFn, ItemImpl, LitStr, ReturnType, Signature, Visibility,
};
extern crate proc_macro;

/// The `handler` attribute makes the associated function into a Bulwark event handler.
///
/// The function must take no parameters and return a `bulwark_wasm_sdk::Result`. It may only be
/// named one of the following:
/// - `on_request`
/// - `on_request_decision`
/// - `on_response_decision`
/// - `on_decision_feedback`
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
        "on_request" | "on_request_decision" | "on_response_decision" | "on_decision_feedback" => {
            output = quote_spanned! {inner_fn.span() =>
                #(#attrs)*
                #sig {
                    // Declares the inlined inner function, calls it, then performs very
                    // basic error handling on the result
                    #[inline(always)]
                    #inner_fn
                    #name().map_err(|e| {
                        eprintln!("error in '{}' handler: {}", #name_str, e);
                        append_tags(["error"]);
                        // Absorbs the error, returning () to match desired signature
                    })
                }
            }
        }
        _ => {
            return syn::Error::new(
                inner_fn.sig.span(),
                "`handler` expects a function named one of:
                
- `on_request`
- `on_request_decision`
- `on_response_decision`
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

/// The `bulwark_plugin` attribute generates default implementations for all handler traits in a module
/// and produces friendly errors for common mistakes.
#[doc(hidden)]
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
            "`bulwark_plugin` requires an impl for the `Handlers` trait",
        )
        .to_compile_error()
        .into();
    }

    let struct_type = &raw_impl.self_ty;

    let mut handlers = vec![
        "on_request",
        "on_request_decision",
        "on_response_decision",
        "on_decision_feedback",
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
            let handler_ident = Ident::new(handler_name, Span::call_site());
            quote! {
                #[handler]
                fn #handler_ident() -> Result {
                    Ok(())
                }
            }
        })
        .collect::<Vec<proc_macro2::TokenStream>>();

    let output = quote! {
        impl bulwark_wasm_sdk::handlers::Handlers for #struct_type {
            #(#new_items)*
            #(#noop_handlers)*
        }
        const _: () = {
            #[doc(hidden)]
            #[export_name = "on-request"]
            #[allow(non_snake_case)]
            unsafe extern "C" fn __export_handlers_on_request() -> i32 {
                handlers::call_on_request::<#struct_type>()
            }
            #[doc(hidden)]
            #[export_name = "on-request-decision"]
            #[allow(non_snake_case)]
            unsafe extern "C" fn __export_handlers_on_request_decision() -> i32 {
                handlers::call_on_request_decision::<#struct_type>()
            }
            #[doc(hidden)]
            #[export_name = "on-response-decision"]
            #[allow(non_snake_case)]
            unsafe extern "C" fn __export_handlers_on_response_decision() -> i32 {
                handlers::call_on_response_decision::<#struct_type>()
            }
            #[doc(hidden)]
            #[export_name = "on-decision-feedback"]
            #[allow(non_snake_case)]
            unsafe extern "C" fn __export_handlers_on_decision_feedback() -> i32 {
                handlers::call_on_decision_feedback::<#struct_type>()
            }
        };
    };

    output.into()
}
