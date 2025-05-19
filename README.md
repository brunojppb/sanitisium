## Sanitisium

Sanitising PDF documents by regenerating the document from scratch

### Development

You must have the [Rust lang](https://www.rust-lang.org/) >= 1.86.0 or above installed.

To install Rust, just follow [the official docs](https://www.rust-lang.org/tools/install) by running the Rustup script:

```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Once this script is done, Rust should be available on your machine.

#### Dependency on PDFium

This project depends on [PDFium](https://github.com/bblanchon/pdfium-binaries), the native c++ library used by Chromium to handle PDFs.

If you are running on Mac, the vendored version of pdfium is already in this repository.
For other platforms, you must include pdfium as an installed dependency on your OS.

## How to test this locally

The PoC expects only one file on the project directory:

- a file named `sample.pdf`

This pdf file can be anything you want so you can test the program. Now run it with:

```shell
cargo run --release
```

You should 10 PDF files as output named `output-n.pdf` with samples of how the Sanitised document looks like.
