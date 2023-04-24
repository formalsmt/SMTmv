# SMT Model Validation

This is a Rust program that reads an SMT formula from a file, and an SMT model from a either file or the standard input, converts both to Isabelle syntax, and then uses Isabelle to check whether the model satisfies the formula by trying to prove it correct. The output is  

- **sat** if the model is valid
- **unsat** if the model is invalid
- **unknown** if the proof could not be finished

## Requirements

This program requires Rust and Isabelle to be installed.

You can download and install Rust from the official [website](https://www.rust-lang.org/tools/install).

To install Isabelle, download the appropriate version for your operating system from the official [website](https://isabelle.in.tum.de/download.html).

After installing Isabelle, you need to clone the [SMT formalization in Isabelle](https://github.com/formalsmt/isabelle_smt) and build a heap image of it.
Open a terminal and run the following commands:

```shell
git clone https://github.com/formalsmt/isabelle_smt
cd isabelle_smt
isabelle build -v -b -d . smt
```

## Usage

To build the program, navigate to the directory containing the `Cargo.toml` file and run `cargo build --release`.
This create the binary `target/release/smtmv`.

Run the program with the following commands:

```text
Usage: smtmv -T <THROOT> <--stdin|--model <MODEL>> <SMT>

Arguments:
  <SMT>  Path to file containing the SMT formula

Options:
      --model <MODEL>  Path to file containing the model (must not be used with --stdin)
      --stdin          Read model from stdin (must not be used with --model)
  -T <THROOT>          Path to the root of the theory directory
  -h, --help           Print help
  -V, --version        Print version
```

### Example

Here is an example to validate a model produce by Z3:

```shell
z3 --model formula.smt  | smtmv -T <isabelle_smt> --stdin formula.smt
```

where `<isabelle_smt>` refers to the directory of the Isabelle formalization.
