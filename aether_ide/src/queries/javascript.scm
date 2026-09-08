; Tree-sitter query file for JavaScript
; Grammar: tree-sitter-javascript

; Function declarations
(function_declaration
  name: (identifier) @function)

; Function expressions
(assignment_expression
  left: (identifier) @function
  right: (function_expression))

; Arrow functions
(arrow_function) @function

; Method definitions
(method_definition
  name: (property_identifier) @function)

; Generator functions
(generator_function_declaration
  name: (identifier) @function)

; Async functions
(statement
  (function_declaration
    name: (identifier) @function))

; Class declarations
(class_declaration
  name: (identifier) @class)

; Class expressions
(class
  name: (identifier) @class)

; Import declarations
(import_statement) @import

; Dynamic imports
(call_expression
  function: (identifier) @_fn
  arguments: (arguments
    (string) @import)
  (#eq? @_fn "import"))

; Comments
(comment) @comment

; Function calls
(call_expression
  function: [
    (identifier) @call
    (member_expression
      property: (property_identifier) @call)
    (parenthesized_expression) @call
    (arrow_function) @call
  ])

; New expressions
(new_expression
  constructor: (identifier) @call)

; String literals
(string) @string
(template_string) @string

; Number literals
(number) @number

; Variable declarations
(variable_declarator
  name: (identifier) @variable)

; Lexical declarations (let/const)
(lexical_declaration
  (variable_declarator
    name: (identifier) @variable))

; Function parameters
(formal_parameters
  (identifier) @parameter)

; Rest parameters
(formal_parameters
  (rest_pattern
    (identifier) @parameter))

; Assignment pattern (default params)
(formal_parameters
  (assignment_pattern
    left: (identifier) @parameter))

; Return statements
(return_statement) @return

; For loops
(for_statement) @loop
(for_in_statement) @loop
for_of_statement: @loop

; While loops
(while_statement) @loop

; Do-while loops
(do_statement) @loop

; Labeled loops
(labeled_statement) @loop

; If statements
(if_statement) @conditional

; Switch statements
(switch_statement) @conditional
(case_clause) @conditional
(default_clause) @conditional

; Ternary expressions
(conditional_expression) @conditional

; Try statements
(try_statement) @try

; Catch clauses
(catch_clause) @try

; Throw statements
(throw_statement) @try

; With statements
(with_statement) @try

; Object patterns in destructuring
(object_pattern) @variable

; Property definitions in classes
(class_static_block) @function

; Getter/Setter methods
(getter) @function
(setter) @function

; Yield expressions (generators)
(yield_expression) @loop

; Await expressions
(await_expression) @try

; Debugger statements
(debugger_statement) @try
