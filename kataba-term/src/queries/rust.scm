; Tree-sitter Rust highlight query for Kataba

; Keywords
[
  "async"
  "await"
  "break"
  "const"
  "continue"
  "crate"
  "else"
  "enum"
  "extern"
  "false"
  "fn"
  "for"
  "if"
  "impl"
  "in"
  "let"
  "loop"
  "match"
  "mod"
  "move"
  "mut"
  "pub"
  "ref"
  "return"
  "self"
  "Self"
  "static"
  "struct"
  "super"
  "trait"
  "true"
  "type"
  "unsafe"
  "use"
  "where"
  "while"
] @keyword

; Types
[
  "bool"
  "char"
  "f32"
  "f64"
  "i8"
  "i16"
  "i32"
  "i64"
  "i128"
  "isize"
  "u8"
  "u16"
  "u32"
  "u64"
  "u128"
  "usize"
  "str"
] @type

; Built-in types and macros
[
  "Box"
  "Option"
  "Result"
  "Some"
  "None"
  "Ok"
  "Err"
  "Vec"
  "String"
  "assert"
  "assert_eq"
  "assert_ne"
  "debug_assert"
  "debug_assert_eq"
  "debug_assert_ne"
  "eprint"
  "eprintln"
  "env"
  "file"
  "format"
  "format_args"
  "include"
  "include_bytes"
  "include_str"
  "line"
  "module_path"
  "option_env"
  "panic"
  "print"
  "println"
  "stringify"
  "todo"
  "unimplemented"
  "unreachable"
  "vec"
  "write"
  "writeln"
] @builtin

; Functions
(function_definition name: (identifier) @function)
(call_function function: (identifier) @function)
(method_receiver) @variable

; Strings
(string_literal) @string
(raw_string_literal) @string
(byte_string_literal) @string
(char_literal) @string

; Numbers
(integer_literal) @number
(float_literal) @number

; Comments
(line_comment) @comment
(block_comment) @comment
