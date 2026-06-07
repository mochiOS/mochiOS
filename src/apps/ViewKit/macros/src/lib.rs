use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, Expr, Ident, LitStr, Token};

use std::fs;
use std::path::PathBuf;

struct ComponentPair {
    name: Ident,
    colon_token: Token![:],
    source: Expr,
}

impl Parse for ComponentPair {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(ComponentPair {
            name: input.parse()?,
            colon_token: input.parse()?,
            source: input.parse()?,
        })
    }
}

struct ComponentsInput {
    pairs: Punctuated<ComponentPair, Token![,]>,
}

impl Parse for ComponentsInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let pairs = Punctuated::<ComponentPair, Token![,]>::parse_terminated(input)?;
        Ok(ComponentsInput { pairs })
    }
}

fn to_snake_case(name: &str) -> String {
    let mut out = String::new();
    let mut prev_was_lower = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() {
                if prev_was_lower && !out.ends_with('_') {
                    out.push('_');
                }
                out.push(ch.to_ascii_lowercase());
                prev_was_lower = false;
            } else {
                out.push(ch.to_ascii_lowercase());
                prev_was_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
            }
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
            prev_was_lower = false;
        } else {
            prev_was_lower = false;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        "component".to_string()
    } else {
        out
    }
}

fn to_pascal_case(name: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if upper {
                out.push(ch.to_ascii_uppercase());
                upper = false;
            } else {
                out.push(ch.to_ascii_lowercase());
            }
        } else {
            upper = true;
        }
    }
    if out.is_empty() {
        "Component".to_string()
    } else {
        out
    }
}

fn normalize_method_name(name: &str) -> String {
    to_snake_case(name)
}

fn extract_source_lit(expr: Expr) -> syn::Result<LitStr> {
    match expr {
        Expr::Lit(lit) => match lit.lit {
            syn::Lit::Str(s) => Ok(s),
            other => Err(syn::Error::new_spanned(
                other,
                "expected string literal or from_str!(\"...\")",
            )),
        },
        Expr::Macro(expr_macro) => {
            let mac = expr_macro.mac;
            if mac.path.is_ident("from_str") || mac.path.is_ident("include_str") {
                syn::parse2::<LitStr>(mac.tokens.clone()).map_err(|_| {
                    syn::Error::new_spanned(
                        mac,
                        "expected from_str!(\"path\") or include_str!(\"path\")",
                    )
                })
            } else {
                Err(syn::Error::new_spanned(
                    mac.path,
                    "expected string literal or from_str!(\"...\")",
                ))
            }
        }
        other => Err(syn::Error::new_spanned(
            other,
            "expected string literal or from_str!(\"...\")",
        )),
    }
}

fn html_metadata(contents: &str) -> (Vec<String>, Vec<String>) {
    use scraper::{Html, Selector};

    let document = Html::parse_document(contents);
    let mut slots = Vec::new();
    let mut content_types = Vec::new();

    if let Ok(slot_sel) = Selector::parse("children, slot") {
        for el in document.select(&slot_sel) {
            if let Some(name) = el.value().attr("name") {
                slots.push(name.to_string());
            } else {
                slots.push("children".to_string());
            }
        }
    }

    if let Ok(content_sel) = Selector::parse("content") {
        for el in document.select(&content_sel) {
            if let Some(t) = el.value().attr("type") {
                content_types.push(t.to_string());
            }
        }
    }

    if let Ok(all_sel) = Selector::parse("*") {
        for el in document.select(&all_sel) {
            if let Some(v) = el.value().attr("data-content-type") {
                content_types.push(v.to_string());
            }
        }
    }

    slots.sort();
    slots.dedup();
    content_types.sort();
    content_types.dedup();
    (slots, content_types)
}

fn make_lit(s: &str, span: proc_macro2::Span) -> LitStr {
    LitStr::new(s, span)
}

#[proc_macro]
pub fn components(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ComponentsInput);

    let mut register_calls = Vec::new();
    let mut meta_consts = Vec::new();
    let mut builder_items = Vec::new();

    for pair in input.pairs.iter() {
        let name_ident = &pair.name;
        let name_str = name_ident.to_string();
        let source_lit = match extract_source_lit(pair.source.clone()) {
            Ok(v) => v,
            Err(e) => return e.to_compile_error().into(),
        };
        let source_value = source_lit.value();

        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
        let resolved_path = PathBuf::from(manifest_dir).join(&source_value);
        let contents = match fs::read_to_string(&resolved_path) {
            Ok(contents) => contents,
            Err(_) => {
                let err = format!("failed to read component file: {}", resolved_path.display());
                return syn::Error::new_spanned(&source_lit, err).to_compile_error().into();
            }
        };

        let (mut slots, mut content_types) = html_metadata(&contents);

        slots.sort();
        slots.dedup();
        content_types.sort();
        content_types.dedup();

        let contents_lit = make_lit(&contents, source_lit.span());
        let name_lit = make_lit(&name_str, name_ident.span());
        let component_mod_name = format!("__viewkit_component_{}", to_snake_case(&name_str));
        let builder_type_name = format!("{}Component", to_pascal_case(&name_str));
        let builder_type_ident = syn::Ident::new(&builder_type_name, name_ident.span());
        let component_mod_ident = syn::Ident::new(&component_mod_name, name_ident.span());

        let has_children = !slots.is_empty();
        let children_method = if has_children {
            quote! {
                pub fn children<I, E>(mut self, elems: I) -> Self
                where
                    I: IntoIterator<Item = E>,
                    E: Into<::viewkit::ui::UIElement>,
                {
                    self.inner = self.inner.children(elems);
                    self
                }
            }
        } else {
            quote! {}
        };

        let content_type_methods: Vec<proc_macro2::TokenStream> = content_types
            .iter()
            .map(|ty| {
                let method_name = normalize_method_name(ty);
                let method_ident = syn::Ident::new(&format!("content_{}", method_name), name_ident.span());
                let type_lit = make_lit(ty, name_ident.span());
                quote! {
                    pub fn #method_ident<T: Into<String>>(mut self, value: T) -> Self {
                        self.inner = self.inner.content_typed(#type_lit, value);
                        self
                    }
                }
            })
            .collect();

        let slot_lits: Vec<LitStr> = slots.iter().map(|s| make_lit(s, name_ident.span())).collect();
        let content_type_lits: Vec<LitStr> = content_types
            .iter()
            .map(|s| make_lit(s, name_ident.span()))
            .collect();
        let contents_lit_builder = contents_lit.clone();
        let contents_lit_meta = contents_lit.clone();
        let contents_lit_register = contents_lit.clone();

        let builder_item = quote! {
            pub mod #component_mod_ident {
                pub const TEMPLATE: &str = #contents_lit_builder;
                pub const HAS_CHILDREN: bool = #has_children;
                pub const CONTENT_TYPES: &[&str] = &[ #( #content_type_lits ),* ];

                #[derive(Clone, Debug)]
                pub struct #builder_type_ident {
                    inner: ::viewkit::ui::ComponentBuilder,
                }

                impl #builder_type_ident {
                    pub fn new() -> Self {
                        Self {
                            inner: ::viewkit::ui::ComponentBuilder::new(#name_lit),
                        }
                    }

                    pub fn attr<K, V>(mut self, key: K, value: V) -> Self
                    where
                        K: Into<String>,
                        V: Into<::serde_json::Value>,
                    {
                        self.inner = self.inner.attr(key, value);
                        self
                    }

                    #children_method
                    #(#content_type_methods)*

                    pub fn into_elem(self) -> ::viewkit::ui::UIElement {
                        self.inner.into_elem()
                    }
                }
            }

            pub use #component_mod_ident::#builder_type_ident;

            pub fn #name_ident() -> #builder_type_ident {
                #builder_type_ident::new()
            }
        };
        builder_items.push(builder_item);

        let reg_call = quote! {
            {
                let tpl: &str = #contents_lit_register;
                if let Err(e) = backend.register_component(#name_str, tpl) {
                    eprintln!("failed to register component '{}': {}", #name_str, e);
                }
            }
        };
        register_calls.push(reg_call);

        let slot_const_ident = syn::Ident::new(
            &format!("VIEWKIT_COMPONENT_{}_SLOTS", name_str.to_uppercase()),
            name_ident.span(),
        );
        let ct_const_ident = syn::Ident::new(
            &format!("VIEWKIT_COMPONENT_{}_CONTENT_TYPES", name_str.to_uppercase()),
            name_ident.span(),
        );
        let tpl_const_ident = syn::Ident::new(
            &format!("VIEWKIT_COMPONENT_{}_TEMPLATE", name_str.to_uppercase()),
            name_ident.span(),
        );

        let meta_const = quote! {
            pub const #tpl_const_ident: &str = #contents_lit_meta;
            pub const #slot_const_ident: &'static [&'static str] = &[ #(#slot_lits),* ];
            pub const #ct_const_ident: &'static [&'static str] = &[ #(#content_type_lits),* ];
        };
        meta_consts.push(meta_const);
    }

    let register_fn = quote! {
        pub fn register_components<B: ::viewkit::backend::ComponentRenderer>(backend: &mut B) {
            #(#register_calls)*
        }
    };

    let expanded = quote! {
        #(#meta_consts)*
        #(#builder_items)*
        #register_fn
    };

    expanded.into()
}


