; Tree-sitter query file for Go
; Grammar: tree-sitter-go

; Function declarations
(function_declaration
  name: (identifier) @function)

; Method declarations
(method_declaration
  name: (field_identifier) @function)

; Function type definitions
(func_type) @function

; Interface method signatures
(method_spec
  name: (field_identifier) @function)

; Type declarations
(type_declaration
  (type_spec
    name: (type_identifier) @class))

; Struct types
(struct_type
  (field_declaration
    name: (field_identifier) @variable))

; Interface types
(interface_type
  (method_spec
    name: (field_identifier) @function))

; Import declarations
(import_declaration) @import

; Import spec
(import_spec
  path: (interpreted_string_literal) @import)

; Dot imports
(import_spec
  name: (dot) @import)

; Comments
(comment) @comment

; Function calls
(call_expression
  function: [
    (identifier) @call
    (selector_expression
      field: (field_identifier) @call)
    (parenthesized_expression) @call
    (func_literal) @call
  ])

; Method calls
(selector_expression
  field: (field_identifier) @call)

; String literals
(interpreted_string_literal) @string
(raw_string_literal) @string
; Byte literals
(raw_string_literal) @string

; Number literals
(int_literal) @number
(float_literal) @number
(imaginary_literal) @number

; Rune literals
(rune_literal) @string

; Variable declarations
(var_declaration
  (var_spec
    name: (identifier) @variable))

; Short variable declarations
(short_var_declaration
  left: (expression_list
    (identifier) @variable))

; Function parameters
(parameter_declaration
  name: (identifier) @parameter)

; Variadic parameters
(variadic_parameter_declaration
  name: (identifier) @parameter)

; Return statements
(return_statement) @return

; For loops
(for_statement) @loop

; For range loops
(range_clause
  left: (expression_list
    (identifier) @variable)) @loop

; While-style for loops
(for_statement
  condition: (expression)) @loop

; If statements
(if_statement) @conditional

; If-init statements
(if_statement
  initialization: (short_var_declaration
    left: (expression_list
      (identifier) @variable))) @conditional

; Switch statements
(expression_switch_statement) @conditional
(type_switch_statement) @conditional

; Select statements (channel operations)
(select_statement) @conditional

; Case clauses
(case_clause) @conditional
(default_clause) @conditional

; Defer statements
(defer_statement) @try

; Go statements (goroutines)
(go_statement) @try

; Recover statements
(recover_statement) @try

; Select cases
(select_case) @conditional

; Labeled statements
(labeled_statement) @loop

; Composite literals
(composite_literal
  type: (type_identifier) @call)

; Make/new builtins
(call_expression
  function: (identifier) @_builtin
  arguments: (argument_list
    (type_identifier) @class)
  (#match? @_builtin "^(make|new)$"))

; Type conversion
(type_conversion_expression
  type: (type_identifier) @call)

; Type assertions
(type_assertion_expression
  type: (type_identifier) @call)

; Channel operations
(send_statement) @loop
(receive_expression) @loop

; Array/slice types
(array_type
  element: (type_identifier) @class)

; Map types
(map_type
  key: (type_identifier) @class
  value: (type_identifier) @class)

; Pointer types
(pointer_type
  type: (type_identifier) @class)

; Interface type (empty)
(interface_type) @class

; Struct type (empty)
(struct_type) @class
