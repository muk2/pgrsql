/-
  Verification.Basic
  Core type definitions for relational algebra formalization.

  This module defines the foundational types used throughout the
  formal verification layer: values, tuples, relations, and predicates.
-/

namespace Verification

/-- SQL-like values with NULL support. Models a subset of PostgreSQL value types. -/
inductive Value where
  | null   : Value
  | bool   : Bool → Value
  | int    : Int → Value
  | string : String → Value
  deriving Repr, BEq, DecidableEq, Inhabited

/-- Column name represented as a string. -/
abbrev ColumnName := String

/-- A schema is an ordered list of column names. -/
abbrev Schema := List ColumnName

/-- A tuple maps column names to values. Represented as a list of (name, value) pairs. -/
abbrev Tuple := List (ColumnName × Value)

/-- A relation is a schema paired with a set of tuples (represented as a list). -/
structure Relation where
  schema : Schema
  tuples : List Tuple
  deriving Repr, BEq, DecidableEq

/-- Three-valued logic for SQL semantics: TRUE, FALSE, or UNKNOWN (for NULL). -/
inductive TVL where
  | true  : TVL
  | false : TVL
  | unknown : TVL
  deriving Repr, BEq, DecidableEq, Inhabited

/-- A predicate over tuples, returning three-valued logic results. -/
def Predicate := Tuple → TVL

/-- An expression that extracts a value from a tuple. -/
def Expression := Tuple → Value

/-- Look up a column value in a tuple. Returns Value.null if not found. -/
def lookupColumn (t : Tuple) (col : ColumnName) : Value :=
  match t.find? (fun p => p.1 == col) with
  | some (_, v) => v
  | none => Value.null

/-- Project a tuple to only the specified columns. -/
def projectTuple (cols : Schema) (t : Tuple) : Tuple :=
  cols.filterMap (fun c =>
    match t.find? (fun p => p.1 == c) with
    | some pair => some pair
    | none => none)

/-- Merge two tuples (for joins). Left tuple takes precedence on conflicts. -/
def mergeTuples (t1 t2 : Tuple) : Tuple :=
  t1 ++ t2.filter (fun p => !t1.any (fun q => q.1 == p.1))

/-- Construct an empty relation with a given schema. -/
def emptyRelation (s : Schema) : Relation :=
  { schema := s, tuples := [] }

/-- TVL conjunction (AND). -/
def TVL.and : TVL → TVL → TVL
  | TVL.true,    TVL.true    => TVL.true
  | TVL.false,   _           => TVL.false
  | _,           TVL.false   => TVL.false
  | _,           _           => TVL.unknown

/-- TVL disjunction (OR). -/
def TVL.or : TVL → TVL → TVL
  | TVL.false,   TVL.false   => TVL.false
  | TVL.true,    _           => TVL.true
  | _,           TVL.true    => TVL.true
  | _,           _           => TVL.unknown

/-- TVL negation (NOT). -/
def TVL.not : TVL → TVL
  | TVL.true    => TVL.false
  | TVL.false   => TVL.true
  | TVL.unknown => TVL.unknown

/-- Convert a TVL to a Bool (true only for TVL.true, matching SQL WHERE semantics). -/
def TVL.isTrue : TVL → Bool
  | .true => Bool.true
  | _     => Bool.false

end Verification
