# `bicycle`

[Handlebars](https://handlebarsjs.com/) with wheels. 🚴🏽‍♀️

...what are the wheels? Well, the `traverse` function!

Built on top of the [`handlebars`](https://crates.io/crates/handlebars) crate, which is probably what you'd prefer using for a web backend.

```rust
use bicycle::Bicycle;

let bike = Bicycle::default();
let rendered = bike.render("Hello {{name}}!", |map| {
    map.insert("name", "Shinji");
}).unwrap();
assert_eq!(rendered, "Hello Shinji!");
```
