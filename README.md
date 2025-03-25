# gather-context

A Rust utility that extracts function call trees from projects for easier analysis and understanding of code flow.

## Features
- Extracts complete function definitions and their call dependencies
- Handles module-aware function resolution
- Supports disambiguation when multiple functions have the same name
- Outputs in a clean, readable format for documentation or prompting

## Installation

Install globally using Cargo:
```bash
cargo install --path .
```

## Usage

```bash
gather-context <project_root> <function_name> [preferred_module] [output_file]
```

### Arguments
- `<project_root>`: Path to the project root directory
- `<function_name>`: Name of the function to analyze
- `[preferred_module]`: Optional module name to disambiguate functions with the same name
- `[output_file]`: Optional file path for output (defaults to stdout)

### Examples

```bash
# Analyze a function and write to file
gather-context ./my-project process_queue transform_writer output.txt

# Analyze a function and print to console
gather-context ./my-project main

# Get help
gather-context --help
```

## Output Format

The output shows each function definition with its complete body:

```
=== path/to/file.rs ===
fn function_name() {
    // Function implementation
}

=== path/to/another/file.rs ===
pub fn another_function() {
    // Another function implementation
}
```

## How It Works

The tool performs static analysis of Rust source files to:

1. Extract all function definitions in the project
2. Build a graph of function call relationships
3. Traverse the graph starting from the specified function
4. Output all visited functions in a clean format

When multiple functions with the same name exist, you can specify a module preference to disambiguate.

## Dependencies

- walkdir: For recursively walking directory structures
- regex: For parsing and extracting function definitions
