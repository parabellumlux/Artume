; Tree-sitter query file for Rust
; Grammar: tree-sitter-rust

; Function definitions
(function_item
  name: (identifier) @function)

; Closures (not captured as @function since they're expressions)

; Impl block methods
(impl_item
  (function_item
    name: (identifier) @function))

; Trait definitions
(trait_item
  name: (type_identifier) @class)

; Impl blocks
(impl_item
  type: (type_identifier) @class)

; Struct definitions
(struct_item
  name: (type_identifier) @class)

; Enum definitions
(enum_item
  name: (type_identifier) @class)

; Type alias definitions
(type_item
  name: (type_identifier) @class)

; Union definitions
(union_item
  name: (type_identifier) @class)

; Module definitions
(mod_item
  name: (identifier) @class)

; Use declarations
(use_declaration) @import

; Extern crate
(extern_crate_declaration) @import

; Comments
(line_comment) @comment
(block_comment) @comment

; Function calls
(call_expression
  function: [
    (identifier) @call
    (field_expression
      field: (field_identifier) @call)
    (method_invocation
      name: (field_identifier) @call)
    (scoped_identifier
      name: (identifier) @call)
    (macro_invocation
      name: (identifier) @call)
  ])

; Macro invocations
(macro_invocation
  name: (identifier) @call)

; String literals
(string_literal) @string
(raw_string_literal) @string
; Byte strings
(byte_string_literal) @string
; CStrings (Rust 1.77+)
(c_string_literal) @string

; Character literals
(char_literal) @string

; Number literals
(integer_literal) @number
(float_literal) @number

; Boolean literals (treated as special numbers)
(boolean_literal) @number

; Let bindings (variable declarations)
(let_declaration
  pattern: (identifier) @variable)

; Mutable let bindings
(let_declaration
  pattern: (mutable_pattern
    (identifier) @variable))

; Destructuring patterns with identifiers
(let_declaration
  (tuple_pattern
    (identifier) @variable))

; Parameter definitions
(parameters
  (parameter
    name: (identifier) @parameter))

; Optional parameters
(optional_parameter
  name: (identifier) @parameter)

; Self parameters
(self_parameter) @parameter

; Return statements
(return_expression) @return
; Implicit return (last expression in block)

; For loops
(for_expression) @loop

; While loops
(while_expression) @loop

; Loop expressions (infinite loops)
(loop_expression) @loop

; If expressions
(if_expression) @conditional

; Match expressions
(match_expression) @conditional

; Match arms
(match_arm
  pattern: (match_pattern) @conditional)

; Try operator (?)
(try_expression) @try

; Err-protected blocks
(unsafe_block) @try

; Macro definitions
(macro_definition
  name: (identifier) @function)

; Function type definitions
(function_type) @function

; Trait implementations
(impl_item
  trait: (type_identifier) @function
  type: (type_identifier) @class)

; Closure parameters
(closure_parameters
  (identifier) @parameter)
(closure_parameters
  (mutable_pattern
    (identifier) @parameter))

; Generic type parameters
(type_parameters
  (type_parameter
    name: (type_identifier) @parameter))

; Const definitions
(const_item
  name: (identifier) @variable)

; Static definitions
(static_item
  name: (identifier) @variable)
