; Tree-sitter query file for Python
; Grammar: tree-sitter-python

; Function definitions
(function_definition
  name: (identifier) @function)

; Async function definitions
(async_function_definition
  name: (identifier) @function)

; Decorated function definitions
(decorated_definition
  (function_definition
    name: (identifier) @function))

; Decorated async function definitions
(decorated_definition
  (async_function_definition
    name: (identifier) @function))

; Lambda functions
(lambda
  parameters: (lambda_parameters) @parameter) @function

; Class definitions
(class_definition
  name: (identifier) @class)

; Decorated class definitions
(decorated_definition
  (class_definition
    name: (identifier) @class))

; Import statements
(import_statement
  module_name: (dotted_name) @import)

; From imports
(from_import_statement
  module_name: (dotted_name) @import)

; Wildcard imports
(from_import_statement
  (wildcard_import) @import)

; Comments
(comment) @comment

; Function calls
(call
  function: [
    (identifier) @call
    (attribute
      attribute: (identifier) @call)
    (subscript
      value: (identifier) @call)
    (parenthesized_expression) @call
  ])

; String literals
(string) @string
(multi_string_concatenation) @string
; F-strings contain string parts
(call
  function: (identifier) @_fn
  arguments: (argument_list
    (string) @string)
  (#match? @_fn "^f$"))

; Number literals
(integer) @number
(float) @number
(imaginary_number) @number

; Variable assignments
(assignment
  left: (identifier) @variable)

; Augmented assignments
(augmented_assignment
  left: (identifier) @variable)

; Walrus operator assignments
(named_expression
  name: (identifier) @variable)

; Parameter definitions
(parameters
  (identifier) @parameter)

; Default parameter values
(default_parameter
  name: (identifier) @parameter)

; Keyword argument in function definition
(keyword_default_parameter
  name: (identifier) @parameter)

; Typed parameters
(typed_parameter
  (identifier) @parameter)

; Return statements
(return_statement) @return

; For loops
(for_statement) @loop
(for_in_statement) @loop

; While loops
(while_statement) @loop

; List comprehensions
(list_comprehension) @loop
; Dict comprehensions
(dictionary_comprehension) @loop
; Set comprehensions
(set_comprehension) @loop
; Generator expressions
(generator_expression) @loop

; If statements
(if_statement) @conditional
; Conditional expressions (ternary)
(conditional_expression) @conditional

; Try statements
(try_statement) @try

; Assert statements (similar to error handling)
(assert_statement) @try

; Raise statements
(raise_statement) @try

; With statements (context managers)
(with_statement) @try

; Match statements (Python 3.10+)
(match_statement) @conditional
(case_clause) @conditional
