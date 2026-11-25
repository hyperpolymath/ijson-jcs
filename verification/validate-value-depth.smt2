; SPDX-License-Identifier: MPL-2.0
; Arithmetic model for the two recursive calls in src/ijson.rs::validate_value.
(set-logic QF_LIA)
(define-fun max-depth () Int 128)
(define-fun rank ((d Int)) Int (- max-depth d))
(declare-const depth Int)

; Depth starts at zero. A container can descend only after the depth guard.
(assert (and (<= 0 depth) (< depth max-depth)))
(define-fun descends ((child Int)) Bool
  (and (<= 0 child) (<= child max-depth) (< (rank child) (rank depth))))

; At the last permitted descent the child has rank zero. Expected: sat.
(push 1)
(assert (= depth (- max-depth 1)))
(assert (= (rank (+ depth 1)) 0))
(check-sat)
(pop 1)

; Can the actual increment violate the invariant or fail to decrease? unsat.
(push 1)
(assert (not (descends (+ depth 1))))
(check-sat)
(pop 1)

; Planted fault: recurse at the same depth. A counterexample must exist: sat.
(assert (not (descends depth)))
(check-sat)
