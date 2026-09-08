; Tree-sitter query file for JSON
; Grammar: tree-sitter-json

; JSON is a data format with limited syntax constructs.
; These queries capture structural elements for analysis.

; Root document
(document) @class

; Object nodes
(object) @class

; Array nodes
(array) @loop

; Object pairs (key-value)
(pair
  key: (string) @variable)

; String values
(string) @string

; Number values
(number) @number

; Boolean values
(true) @number
(false) @number

; Null values
(null) @variable

; String keys in objects
(pair
  key: (string) @class)

; Nested objects (for structure visualization)
(pair
  value: (object) @class)

; Nested arrays (for structure visualization)
(pair
  value: (array) @loop)

; Array elements (for structure visualization)
(array
  (string) @string)
(array
  (number) @number)
(array
  (object) @class)
(array
  (true) @number)
(array
  (false) @number)
(array
  (null) @variable)
