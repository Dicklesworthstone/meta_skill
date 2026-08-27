# Reasons for the forking (or we should say reseting)

For the sad reasons that we all know about, bincode has leaved us because of those who has doxxing the original developers.

Apich Orgnisation strongly opsite any type of doxxing and will not tolerate it. (We also suffered from these kind of events before but only for our personal interest reasons) But in any sense, we respect the original developers and their work.

For the dependency issue of RSSN and the bigger Rust community, we have decided to fork the project and continue the development. The project will be renamed to `bincode-next` and will be hosted on GitHub and Codeberg.

# Special disclaimer

1. We fully respect the any copyright notice from the original developers.
2. We will not tolerate any form of doxxing or harassment.
3. We will not tolerate any form of discrimination or hate speech.
4. We will not tolerate any form of plagiarism or copyright infringement.
5. As one of the mission of Apich, we will continue to test the edges of the current AI system assisted coding and development. Discussions on that is welcomed but only without hate.

# Original develop teams' last messages

Due to a doxxing incident bincode development has officially ceased and will not resume. Updates will only be pushed to the in the unlikely event of CVEs. Do not contact us for any other reason.

To those of you who bothered doxxing us. Go touch grass and maybe for once consider your actions have consequences for real people.

Fuck off and worst regards, The Bincode Team

# Virtue, a sinless derive macro helper

## Goals

- Zero dependencies, so fast compile times
- No other dependencies needed
- Declarative code generation
- As much typesystem checking as possible
- Build for modern rust: 1.57 and up
- Build with popular crates in mind:
  - [bincode](https://docs.rs/bincode)
- Will always respect semver. Minor releases will never have:
  - Breaking API changes
  - MSRV changes

## Example

```rust
use virtue::prelude::*;

#[proc_macro_derive(YourDerive, attributes(some, attributes, go, here))]
pub fn derive_your_derive(input: TokenStream) -> TokenStream {
    derive_your_derive_inner(input)
        .unwrap_or_else(|error| error.into_token_stream())
}

fn derive_your_derive_inner(input: TokenStream) -> Result<TokenStream> {
    // Parse the struct or enum you want to implement a derive for
    let parse = Parse::new(input)?;
    // Get a reference to the generator
    let (mut generator, body) = parse.into_generator();
    match body {
        Body::Struct(body) => {
            // Implement your struct body here
            // See `Generator` for more information
            generator.impl_for("YourTrait")?
                    .generate_fn("your_fn")
                    .with_self_arg(FnSelfArg::RefSelf)
                    .body(|fn_body| {
                        fn_body.push_parsed("println!(\"Hello world\");");
                    })?;
        },
        Body::Enum(body) => {
            // Implement your enum body here
            // See `Generator` for more information
            generator.impl_for("YourTrait")?
                    .generate_fn("your_fn")
                    .with_self_arg(FnSelfArg::RefSelf)
                    .body(|fn_body| {
                        fn_body.push_parsed("println!(\"Hello world\");");
                    })?;
        },
    }
    generator.finish()
}
```

Will generate

```ignore
impl YourTrait for <Struct or Enum> {
    fn your_fn(&self) { // .generate_fn("your_fn").with_self_arg(FnSelfArg::RefSelf)
        println!("Hello world"); // fn_body.push_parsed(...)
    }
}
```
