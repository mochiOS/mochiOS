use proc_macro::TokenStream;

#[proc_macro]
pub fn driver(input: TokenStream) -> TokenStream {
    let ty = input
        .to_string()
        .trim()
        .trim_end_matches(';')
        .trim()
        .to_string();
    if ty.is_empty() {
        return "compile_error!(\"plugkit::driver! expects a driver type\");"
            .parse()
            .expect("compile_error token stream");
    }

    let expanded = format!(
        r#"
        #[unsafe(no_mangle)]
        pub extern "C" fn plugkit_driver_entry() -> *const ::plugkit::DriverDescriptor {{
            ::plugkit::driver_descriptor::<{ty}>() as *const ::plugkit::DriverDescriptor
        }}
        "#
    );
    expanded
        .parse()
        .expect("generated plugkit driver entry should parse")
}
