use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote_spanned;
use syn::{
    parse_macro_input, parse_quote, punctuated::Punctuated, spanned::Spanned, Attribute, Ident,
    ItemFn, LitStr, ReturnType, Signature, Visibility,
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
    let (mod_name, trait_name, struct_name, export_name, underscored_export_name, call_name) =
        export_info(&raw_handler);
    let (name, inner_fn) = inner_fn_info(raw_handler);

    let output;

    match name.to_string().as_str() {
        "on_request" | "on_request_decision" | "on_response_decision" | "on_decision_feedback" => {
            output = quote_spanned! {inner_fn.span() =>
                // Guest implementation only needs an empty struct, request context is held on the host
                struct #struct_name;
                // Implements the handler trait and its corresponding function
                impl #mod_name::#trait_name for #struct_name {
                    #(#attrs)*
                    #sig {
                        // Declares the inlined inner function, calls it, then performs very
                        // basic error handling on the result
                        #[inline(always)]
                        #inner_fn
                        #name().map_err(|e| {
                            println!("error during plugin execution: {}", e);
                            // TODO: once function is implemented
                            //append_tags(["error"]);
                            // Absorbs the error, returning () to match desired signature
                        })
                    }
                }
                const _: () = {
                    #[doc(hidden)]
                    #[export_name = #export_name]
                    #[allow(non_snake_case)]
                    unsafe extern "C" fn #underscored_export_name () -> i32 {
                        #mod_name::#call_name::<#struct_name>()
                    }
                };
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

/// Generates the identifiers and values needed to correctly produce wit-bindgen boilerplate.
///
/// Returns a 6-tuple of supporting identifiers and values that will be injected into our macro's
/// template.
fn export_info(inner_handler: &ItemFn) -> (Ident, Ident, Ident, LitStr, Ident, Ident) {
    let name = inner_handler.sig.ident.clone();
    let mod_name = Ident::new(
        (name.to_string().replacen("on_", "", 1) + "_handler").as_str(),
        Span::call_site(),
    );
    let trait_name = Ident::new(
        mod_name.to_string().to_case(Case::UpperCamel).as_str(),
        Span::call_site(),
    );
    let struct_name = Ident::new(
        format!("{}{}", "Plugin", trait_name).as_str(),
        Span::call_site(),
    );
    let export_name = LitStr::new(
        name.to_string().to_case(Case::Kebab).as_str(),
        Span::call_site(),
    );
    let underscored_export_name = Ident::new(
        format!("__export_{}_{}", mod_name, name).as_str(),
        Span::call_site(),
    );
    let call_name = Ident::new(format!("call_{}", name).as_str(), Span::call_site());
    (
        mod_name,
        trait_name,
        struct_name,
        export_name,
        underscored_export_name,
        call_name,
    )
}
