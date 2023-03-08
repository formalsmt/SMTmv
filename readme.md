# SMT Model Validation

This is a Rust program that reads an SMT formula from a file, and an SMT model from a either file or the standard input, converts both to Isabelle syntax, and then uses Isabelle to check whether the model satisfies the formula by trying to prove it correct. The output is  

- **sat** if the model is valid
- **unsat** if the model is invalid
- **unknown** if the proof could not be finished

## Requirements

This program requires Rust and Isabelle to be installed.

You can download and install Rust from the official [website](https://www.rust-lang.org/tools/install).

To install Isabelle, download the appropriate version for your operating system from the official [website](https://isabelle.in.tum.de/download.html).

After installing Isabelle, you need to clone the [SMT formalization in Isabelle](https://github.com/lotzk/isabelle_smt) and build a heap image of it.
Open a terminal and run the following commands:

```shell
git clone https://github.com/lotzk/isabelle_smt
cd isabelle_smt
isabelle build -o quick_and_dirty -v -b -d . smt
```

## Usage

To build the program, navigate to the directory containing the Cargo.toml file and run `cargo build --release`.
This create the binary `target/release/smt_mv`.

Run the program with the following commands:

```text
./smt_mv -- [OPTIONS]

Replace [OPTIONS] with of the following options:

--model [FILE]: Read the SMT model from FILE instead of standard input (must not be used with --stdin).
--stdin: Instead of reading the model from a file, read it from standard input (must not be used with --model).
--T [DIR]: Needs to point to the root directory of the Isabelle SMT formalization (see above).
--smt [FILE]: FILE containing the SMT formula.
```

### Example

Here is an example to validate a model produce by Z3:

```shell
z3 --model formula.smt  | smt_mv -T <isabelle_smt> --stdin formula.smt
```

where `<isabelle_smt>` refers to the directory of the Isabelle formalization.
