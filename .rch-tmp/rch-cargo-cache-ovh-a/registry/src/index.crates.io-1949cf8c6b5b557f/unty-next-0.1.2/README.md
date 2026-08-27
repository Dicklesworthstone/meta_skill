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

# Virtue

A crate that allows you to mostly-safely cast one type into another type.

This is mostly useful for generic functions, e.g.

```rs
pub fn foo<S>(s: S) {
    if let Ok(a) = unsafe { unty::<S, u8>(s) } {
        println!("It is an u8 with value {a}");
    } else {
        println!("it is not an u8");
    }
}
foo(10u8); // will print "it is an u8"
foo("test"); // will print "it is not an u8"
```

This operation is still unsafe because it allows you to extend lifetimes. There currently is not a way to prevent this

```rs
if let Ok(str) = unsafe { unty::<&'a str, &'static str>(input) } {
    // the compiler may now light your PC on fire
}
```

# License

This crate is dual licenced MIT and Apache-2.0, at your own leisure
