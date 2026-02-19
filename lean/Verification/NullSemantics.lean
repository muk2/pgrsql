/-
  Verification.NullSemantics
  Formalization of SQL NULL and three-valued logic (3VL).

  SQL's three-valued logic is one of the most subtle aspects of query
  semantics. Incorrect handling of NULLs is a major source of optimizer
  bugs. This module formalizes 3VL truth tables and proves key properties.
-/

import Verification.Basic

namespace Verification

/-! ## Three-Valued Logic Truth Tables

  These theorems verify that our TVL implementation matches the SQL standard
  truth tables for AND, OR, and NOT. -/

/-- AND truth table verification. -/
theorem tvl_and_truth_table :
    TVL.and TVL.true TVL.true = TVL.true ∧
    TVL.and TVL.true TVL.false = TVL.false ∧
    TVL.and TVL.true TVL.unknown = TVL.unknown ∧
    TVL.and TVL.false TVL.true = TVL.false ∧
    TVL.and TVL.false TVL.false = TVL.false ∧
    TVL.and TVL.false TVL.unknown = TVL.false ∧
    TVL.and TVL.unknown TVL.true = TVL.unknown ∧
    TVL.and TVL.unknown TVL.false = TVL.false ∧
    TVL.and TVL.unknown TVL.unknown = TVL.unknown := by
  exact ⟨rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl⟩

/-- OR truth table verification. -/
theorem tvl_or_truth_table :
    TVL.or TVL.true TVL.true = TVL.true ∧
    TVL.or TVL.true TVL.false = TVL.true ∧
    TVL.or TVL.true TVL.unknown = TVL.true ∧
    TVL.or TVL.false TVL.true = TVL.true ∧
    TVL.or TVL.false TVL.false = TVL.false ∧
    TVL.or TVL.false TVL.unknown = TVL.unknown ∧
    TVL.or TVL.unknown TVL.true = TVL.true ∧
    TVL.or TVL.unknown TVL.false = TVL.unknown ∧
    TVL.or TVL.unknown TVL.unknown = TVL.unknown := by
  exact ⟨rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl⟩

/-- NOT truth table verification. -/
theorem tvl_not_truth_table :
    TVL.not TVL.true = TVL.false ∧
    TVL.not TVL.false = TVL.true ∧
    TVL.not TVL.unknown = TVL.unknown := by
  exact ⟨rfl, rfl, rfl⟩

/-! ## NULL Propagation Properties -/

/-- IS TRUE semantics: only TVL.true passes. -/
theorem isTrue_semantics :
    TVL.isTrue TVL.true = true ∧
    TVL.isTrue TVL.false = false ∧
    TVL.isTrue TVL.unknown = false := by
  exact ⟨rfl, rfl, rfl⟩

/-- AND with UNKNOWN never produces TRUE (unless both are TRUE). -/
theorem and_unknown_not_true (a : TVL) :
    TVL.and a TVL.unknown ≠ TVL.true := by
  cases a <;> simp [TVL.and]

/-- OR with TRUE always produces TRUE regardless of NULLs. -/
theorem or_true_absorb (a : TVL) :
    TVL.or TVL.true a = TVL.true := by
  cases a <;> rfl

/-- AND with FALSE always produces FALSE regardless of NULLs. -/
theorem and_false_absorb (a : TVL) :
    TVL.and TVL.false a = TVL.false := by
  cases a <;> rfl

/-! ## TVL Algebraic Properties -/

/-- TVL.and is associative. -/
theorem tvl_and_assoc (a b c : TVL) :
    TVL.and (TVL.and a b) c = TVL.and a (TVL.and b c) := by
  cases a <;> cases b <;> cases c <;> rfl

/-- TVL.or is associative. -/
theorem tvl_or_assoc (a b c : TVL) :
    TVL.or (TVL.or a b) c = TVL.or a (TVL.or b c) := by
  cases a <;> cases b <;> cases c <;> rfl

/-- TVL.and distributes over TVL.or. -/
theorem tvl_and_or_distrib (a b c : TVL) :
    TVL.and a (TVL.or b c) = TVL.or (TVL.and a b) (TVL.and a c) := by
  cases a <;> cases b <;> cases c <;> rfl

/-- TVL.or distributes over TVL.and. -/
theorem tvl_or_and_distrib (a b c : TVL) :
    TVL.or a (TVL.and b c) = TVL.and (TVL.or a b) (TVL.or a c) := by
  cases a <;> cases b <;> cases c <;> rfl

end Verification
