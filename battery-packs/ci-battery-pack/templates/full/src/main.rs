{%- if all or binary_release -%}
fn main() {
    println!("Hello from {}!", env!("CARGO_PKG_NAME"));
}
{% endif %}