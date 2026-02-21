/-
  Verification.Examples
  Executable examples demonstrating the relational algebra definitions.

  These examples serve as both documentation and executable tests,
  showing how the formal definitions correspond to concrete SQL operations.
-/

import Verification.Basic
import Verification.Operations

namespace Verification.Examples

/-! ## Example Relations -/

/-- Example: employees table
    | name    | dept | salary |
    |---------|------|--------|
    | Alice   | eng  | 100    |
    | Bob     | eng  | 90     |
    | Charlie | hr   | 80     |
-/
def employees : Relation :=
  { schema := ["name", "dept", "salary"]
    tuples := [
      [("name", Value.string "Alice"),   ("dept", Value.string "eng"), ("salary", Value.int 100)],
      [("name", Value.string "Bob"),     ("dept", Value.string "eng"), ("salary", Value.int 90)],
      [("name", Value.string "Charlie"), ("dept", Value.string "hr"),  ("salary", Value.int 80)]
    ] }

/-- Example: departments table
    | dept | location |
    |------|----------|
    | eng  | SF       |
    | hr   | NYC      |
-/
def departments : Relation :=
  { schema := ["dept", "location"]
    tuples := [
      [("dept", Value.string "eng"), ("location", Value.string "SF")],
      [("dept", Value.string "hr"),  ("location", Value.string "NYC")]
    ] }

/-! ## Example Predicates -/

/-- WHERE dept = 'eng' -/
def isEng : Predicate := fun t =>
  if lookupColumn t "dept" == Value.string "eng"
  then TVL.true
  else TVL.false

/-- WHERE salary > 85 (simplified integer comparison) -/
def salaryAbove85 : Predicate := fun t =>
  match lookupColumn t "salary" with
  | Value.int n => if n > 85 then TVL.true else TVL.false
  | Value.null  => TVL.unknown
  | _           => TVL.false

/-! ## Selection Examples -/

/-- SELECT * FROM employees WHERE dept = 'eng' → 2 rows (Alice, Bob) -/
example : (Verification.select isEng employees).tuples.length = 2 := by native_decide

/-- SELECT * FROM employees WHERE salary > 85 → 2 rows (Alice, Bob) -/
example : (Verification.select salaryAbove85 employees).tuples.length = 2 := by native_decide

/-! ## Projection Examples -/

/-- SELECT name FROM employees → 3 rows -/
example : (project ["name"] employees).tuples.length = 3 := by native_decide

/-! ## Combined Operations -/

/-- SELECT name FROM employees WHERE dept = 'eng'
    π_name(σ_{dept='eng'}(employees)) → 2 rows -/
example : (project ["name"] (Verification.select isEng employees)).tuples.length = 2 := by
  native_decide

/-! ## Cross Product Example -/

/-- employees × departments should have 3 × 2 = 6 tuples -/
example : (crossProduct employees departments).tuples.length = 6 := by native_decide

/-! ## Union Example -/

def moreEmployees : Relation :=
  { schema := ["name", "dept", "salary"]
    tuples := [
      [("name", Value.string "Diana"), ("dept", Value.string "hr"), ("salary", Value.int 95)]
    ] }

/-- employees ∪ moreEmployees should have 3 + 1 = 4 tuples -/
example : (Verification.union employees moreEmployees).tuples.length = 4 := by native_decide

/-! ## NULL Handling Example -/

/-- Table with NULL values -/
def withNulls : Relation :=
  { schema := ["name", "score"]
    tuples := [
      [("name", Value.string "Alice"), ("score", Value.int 95)],
      [("name", Value.string "Bob"),   ("score", Value.null)],
      [("name", Value.string "Carol"), ("score", Value.int 80)]
    ] }

/-- WHERE score > 85: NULL scores are filtered out (UNKNOWN → not selected) -/
def scoreAbove85 : Predicate := fun t =>
  match lookupColumn t "score" with
  | Value.int n => if n > 85 then TVL.true else TVL.false
  | Value.null  => TVL.unknown
  | _           => TVL.false

/-- Only Alice passes (Bob's NULL → UNKNOWN → filtered out, Carol's 80 → FALSE) -/
example : (Verification.select scoreAbove85 withNulls).tuples.length = 1 := by native_decide

end Verification.Examples
