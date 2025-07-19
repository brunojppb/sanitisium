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

If you are running on Mac (ARM) or Linux (x64), the vendored version of pdfium is already in this repository.
For other platforms, you must include pdfium as an installed dependency on your OS.

## How to test this locally

The binary expects one argument with the path to the PDF file input.
This pdf file can be anything you want so you can test the program.

Optionally, you can give an output file path to save the final result.

Here is how to run it:

```shell
cargo run -p sanitisium cli --release -- sample.pdf --output my_clean_file.pdf
```

To check all the CLI arguments available, run with the `--help` flag:

```shell
cargo run -p sanitisium cli --release -- --help
# You should see the following output

Tool for regenerating PDFs

Usage: sanitisium-cli [OPTIONS] <INPUT>

Arguments:
  <INPUT>  The PDF file to sanitise

Options:
  -o, --output <OUTPUT>  Output path for the sanitised PDF. Defaults to the input filename prefixed with 'regenerated_'
  -h, --help             Print help
  -V, --version          Print version
```
