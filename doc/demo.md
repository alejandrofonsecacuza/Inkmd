# inkmd

A **fast**, *minimal* Markdown renderer for your terminal.

## Code

```rust
fn main() {
    let name = "inkmd";
    println!("Hello from {name}!"); // highlighted
}
```

### Tables

| Feature    | Kitty | Other terminals |
|:-----------|:-----:|----------------:|
| Images     |  yes  |       alt text  |
| Big titles |  yes  |    bold + color |

#### Lists and quotes

- [x] CommonMark + GFM
- [ ] Your next README
  - nested, with `inline code` and [links](https://github.com)

> [!TIP]
> Try `inkmd --help` or `man inkmd`.

![Sample image](../tests/fixtures/assets/sample.png)
