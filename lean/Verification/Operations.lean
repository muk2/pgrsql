/-
  Verification.Operations
  Relational algebra operations.

  Defines the core relational algebra operators:
  - Selection (σ): Filter tuples by predicate
  - Projection (π): Select specific columns
  - Join (⨝): Combine relations on a predicate
  - Rename (ρ): Rename columns
  - Union (∪): Set union of tuples
  - Intersection (∩): Set intersection
  - Difference (−): Set difference
  - Cross product (×): Cartesian product
-/

import Verification.Basic

namespace Verification

/-! ## Selection (σ) -/

/-- Selection: filter tuples where the predicate evaluates to TRUE.
    σ_p(R) = { t ∈ R | p(t) = TRUE } -/
def select (p : Predicate) (r : Relation) : Relation :=
  { schema := r.schema
    tuples := r.tuples.filter (fun t => (p t).isTrue) }

/-! ## Projection (π) -/

/-- Projection: keep only the specified columns.
    π_cols(R) = { t[cols] | t ∈ R } -/
def project (cols : Schema) (r : Relation) : Relation :=
  { schema := cols
    tuples := r.tuples.map (projectTuple cols) }

/-! ## Cross Product (×) -/

/-- Cartesian product of two relations.
    R × S = { merge(t1, t2) | t1 ∈ R, t2 ∈ S } -/
def crossProduct (r s : Relation) : Relation :=
  { schema := r.schema ++ s.schema
    tuples := r.tuples.flatMap (fun t1 =>
      s.tuples.map (fun t2 => mergeTuples t1 t2)) }

/-! ## Join (⨝) -/

/-- Theta join: cross product followed by selection.
    R ⨝_p S = σ_p(R × S) -/
def join (p : Predicate) (r s : Relation) : Relation :=
  select p (crossProduct r s)

/-- Natural join: join on all common column names with equality. -/
def naturalJoin (r s : Relation) : Relation :=
  let commonCols := r.schema.filter (fun c => s.schema.contains c)
  let pred : Predicate := fun t =>
    if commonCols.all (fun c =>
      let v1 := lookupColumn t c
      let v2 := lookupColumn t c
      v1 == v2 && v1 != Value.null)
    then TVL.true
    else TVL.false
  join pred r s

/-! ## Rename (ρ) -/

/-- Rename a column in a relation.
    ρ_{new/old}(R) -/
def rename (oldName newName : ColumnName) (r : Relation) : Relation :=
  { schema := r.schema.map (fun c => if c == oldName then newName else c)
    tuples := r.tuples.map (fun t =>
      t.map (fun (c, v) => if c == oldName then (newName, v) else (c, v))) }

/-! ## Set Operations -/

/-- Union of two relations (bag semantics — concatenation of tuples).
    R ∪ S -/
def union (r s : Relation) : Relation :=
  { schema := r.schema
    tuples := r.tuples ++ s.tuples }

/-- Intersection: tuples present in both relations. -/
def intersection (r s : Relation) : Relation :=
  { schema := r.schema
    tuples := r.tuples.filter (fun t => s.tuples.any (fun t2 => t == t2)) }

/-- Difference: tuples in R but not in S.
    R − S -/
def difference (r s : Relation) : Relation :=
  { schema := r.schema
    tuples := r.tuples.filter (fun t => !s.tuples.any (fun t2 => t == t2)) }

/-! ## Compound Predicates -/

/-- Conjunction of two predicates. -/
def predAnd (p q : Predicate) : Predicate :=
  fun t => TVL.and (p t) (q t)

/-- Disjunction of two predicates. -/
def predOr (p q : Predicate) : Predicate :=
  fun t => TVL.or (p t) (q t)

/-- Negation of a predicate. -/
def predNot (p : Predicate) : Predicate :=
  fun t => TVL.not (p t)

/-- A predicate that always returns TRUE. -/
def predTrue : Predicate := fun _ => TVL.true

/-- A predicate that always returns FALSE. -/
def predFalse : Predicate := fun _ => TVL.false

/-! ## Algebraic Expression Type -/

/-- Relational algebra expression as an inductive type.
    This allows representing query plans as data. -/
inductive RelExpr where
  | base    : Relation → RelExpr
  | sel     : Predicate → RelExpr → RelExpr
  | proj    : Schema → RelExpr → RelExpr
  | cross   : RelExpr → RelExpr → RelExpr
  | join    : Predicate → RelExpr → RelExpr → RelExpr
  | rname   : ColumnName → ColumnName → RelExpr → RelExpr
  | union   : RelExpr → RelExpr → RelExpr
  | inter   : RelExpr → RelExpr → RelExpr
  | diff    : RelExpr → RelExpr → RelExpr

/-- Evaluate a relational algebra expression to a concrete relation. -/
def eval : RelExpr → Relation
  | .base r         => r
  | .sel p e        => Verification.select p (eval e)
  | .proj cols e    => Verification.project cols (eval e)
  | .cross e1 e2    => crossProduct (eval e1) (eval e2)
  | .join p e1 e2   => Verification.join p (eval e1) (eval e2)
  | .rname old new e => rename old new (eval e)
  | .union e1 e2    => Verification.union (eval e1) (eval e2)
  | .inter e1 e2    => intersection (eval e1) (eval e2)
  | .diff e1 e2     => difference (eval e1) (eval e2)

end Verification
