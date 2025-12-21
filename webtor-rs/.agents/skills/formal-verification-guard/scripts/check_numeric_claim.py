#!/usr/bin/env python3
"""
check_numeric_claim.py

Utility script to mechanically sanity-check simple numeric or functional claims.

Usage examples:
    python scripts/check_numeric_claim.py --test-sum
    python scripts/check_numeric_claim.py --test-sum --samples 1000
    python scripts/check_numeric_claim.py --test-expression "x**2 + y**2" --expected "z**2" --domain "pythagorean"

This script is intentionally minimal and should be adapted per project.
"""

import argparse
import math
import random
import sys
from typing import Callable, List, Tuple, Any


def triangular_number(n: int) -> int:
    """Closed-form formula for sum of 1..n"""
    return n * (n + 1) // 2


def brute_force_sum(n: int) -> int:
    """Brute force sum of 1..n"""
    return sum(range(1, n + 1))


def test_triangular_identity(samples: int = 20, max_n: int = 10_000) -> List[Tuple[int, int, int]]:
    """Test that triangular_number matches brute_force_sum."""
    failures = []
    for _ in range(samples):
        n = random.randint(0, max_n)
        lhs = triangular_number(n)
        rhs = brute_force_sum(n)
        if lhs != rhs:
            failures.append((n, lhs, rhs))
    return failures


def check_commutativity(op: Callable[[Any, Any], Any], domain: List[Any], samples: int = 100) -> List[Tuple[Any, Any]]:
    """Check if an operation is commutative on a domain."""
    failures = []
    for _ in range(samples):
        a = random.choice(domain)
        b = random.choice(domain)
        try:
            if op(a, b) != op(b, a):
                failures.append((a, b))
        except Exception:
            failures.append((a, b))
    return failures


def check_associativity(op: Callable[[Any, Any], Any], domain: List[Any], samples: int = 100) -> List[Tuple[Any, Any, Any]]:
    """Check if an operation is associative on a domain."""
    failures = []
    for _ in range(samples):
        a = random.choice(domain)
        b = random.choice(domain)
        c = random.choice(domain)
        try:
            lhs = op(op(a, b), c)
            rhs = op(a, op(b, c))
            if lhs != rhs:
                failures.append((a, b, c))
        except Exception:
            failures.append((a, b, c))
    return failures


def search_counterexample_predicate(
    predicate: Callable[[int], bool],
    start: int = 0,
    end: int = 10000,
    description: str = "predicate"
) -> List[int]:
    """Search for counterexamples where predicate returns False."""
    counterexamples = []
    for n in range(start, end + 1):
        try:
            if not predicate(n):
                counterexamples.append(n)
                if len(counterexamples) >= 10:
                    break
        except Exception as e:
            print(f"[WARN] Exception at n={n}: {e}")
    return counterexamples


def test_quadratic_formula(samples: int = 100) -> List[dict]:
    """Test quadratic formula on random quadratics."""
    failures = []
    for _ in range(samples):
        a = random.uniform(-10, 10)
        if abs(a) < 0.01:
            a = 1.0
        b = random.uniform(-10, 10)
        c = random.uniform(-10, 10)
        
        discriminant = b**2 - 4*a*c
        if discriminant < 0:
            continue
        
        x1 = (-b + math.sqrt(discriminant)) / (2*a)
        x2 = (-b - math.sqrt(discriminant)) / (2*a)
        
        result1 = a*x1**2 + b*x1 + c
        result2 = a*x2**2 + b*x2 + c
        
        tolerance = 1e-9
        if abs(result1) > tolerance or abs(result2) > tolerance:
            failures.append({
                "a": a, "b": b, "c": c,
                "x1": x1, "x2": x2,
                "result1": result1, "result2": result2
            })
    return failures


def main():
    parser = argparse.ArgumentParser(
        description="Simple numeric / functional sanity checks for mathematical claims."
    )
    parser.add_argument(
        "--test-sum", action="store_true",
        help="Test the triangular number identity: sum(1..n) = n(n+1)/2"
    )
    parser.add_argument(
        "--test-quadratic", action="store_true",
        help="Test the quadratic formula on random quadratics"
    )
    parser.add_argument(
        "--samples", type=int, default=100,
        help="Number of random samples for tests (default: 100)"
    )
    parser.add_argument(
        "--search-counterexample", type=str, metavar="EXPR",
        help="Search for counterexamples to a Python expression involving 'n' (e.g., 'n**2 >= 0')"
    )
    parser.add_argument(
        "--range", type=str, default="0,1000",
        help="Range for counterexample search as 'start,end' (default: 0,1000)"
    )
    
    args = parser.parse_args()
    
    if args.test_sum:
        print(f"Testing triangular number identity with {args.samples} samples...")
        failures = test_triangular_identity(samples=args.samples)
        if failures:
            print(f"[FAIL] Found {len(failures)} counterexamples:")
            for n, lhs, rhs in failures[:5]:
                print(f"  n={n}: formula={lhs}, brute_force={rhs}")
            sys.exit(1)
        else:
            print(f"[OK] No counterexamples found in {args.samples} random tests.")
        return
    
    if args.test_quadratic:
        print(f"Testing quadratic formula with {args.samples} samples...")
        failures = test_quadratic_formula(samples=args.samples)
        if failures:
            print(f"[FAIL] Found {len(failures)} cases with residual > tolerance:")
            for f in failures[:3]:
                print(f"  a={f['a']:.4f}, b={f['b']:.4f}, c={f['c']:.4f}")
                print(f"    x1={f['x1']:.6f} -> {f['result1']:.2e}")
                print(f"    x2={f['x2']:.6f} -> {f['result2']:.2e}")
            sys.exit(1)
        else:
            print(f"[OK] Quadratic formula verified for {args.samples} random quadratics.")
        return
    
    if args.search_counterexample:
        try:
            start, end = map(int, args.range.split(","))
        except ValueError:
            print(f"[ERROR] Invalid range format: {args.range}. Use 'start,end'")
            sys.exit(1)
        
        expr = args.search_counterexample
        print(f"Searching for counterexamples to: {expr}")
        print(f"Range: [{start}, {end}]")
        
        def predicate(n: int) -> bool:
            return eval(expr, {"n": n, "math": math})
        
        counterexamples = search_counterexample_predicate(predicate, start, end, expr)
        
        if counterexamples:
            print(f"[FAIL] Found counterexamples at n = {counterexamples}")
            sys.exit(1)
        else:
            print(f"[OK] No counterexamples found in range [{start}, {end}]")
        return
    
    print("""
No specific test selected.

Available tests:
  --test-sum              Test triangular number identity
  --test-quadratic        Test quadratic formula
  --search-counterexample Search for counterexamples to an expression

Examples:
  python check_numeric_claim.py --test-sum --samples 1000
  python check_numeric_claim.py --search-counterexample "n % 2 == 0 or n % 2 == 1" --range "0,100"
  python check_numeric_claim.py --search-counterexample "n**2 >= n" --range "-10,100"

Extend this script with domain-specific checks as needed.
""".strip())


if __name__ == "__main__":
    main()
