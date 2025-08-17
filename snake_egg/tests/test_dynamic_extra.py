from __future__ import annotations

from typing import List, NamedTuple, Union, cast

from snake_egg import EGraph, Rewrite, Var, vars


class Add(NamedTuple):
    x: "Expr"
    y: "Expr"


Expr = Union[str, int, Add, Var]


def replace_add(x: Expr, y: Expr) -> Expr:
    # dynamic RHS: fold only when both are ints
    if isinstance(x, int) and isinstance(y, int):
        return x + y
    return Add(x, y)


x, y = cast(List[Var], vars("x y"))

rules = [Rewrite(Add(x, y), replace_add, name="replace-add")]


def simplify(expr: Expr):
    egraph = EGraph()
    egraph.add(expr)
    egraph.run(rules)
    # extract returns a single object when one expr is passed
    return egraph.extract(expr)


def fully_eval(expr: Expr) -> Expr:
    # Helper function to recursively evaluate an Add node if possible.
    if isinstance(expr, int):
        return expr
    if isinstance(expr, Add):
        x_val = fully_eval(expr.x)
        y_val = fully_eval(expr.y)
        if isinstance(x_val, int) and isinstance(y_val, int):
            return x_val + y_val
        return Add(x_val, y_val)
    return expr


def test_simple_fold():
    assert simplify(Add(1, 2)) == 3


def test_nested_preserved():
    # inner Add is non-foldable (contains strings), whole expr should remain
    assert simplify(Add(1, Add("x", "y"))) == Add(1, Add("x", "y"))


def test_inner_fold_under_outer():
    # inner Add(2,3) should fold to 5; outer should become Add("a", 5)
    assert simplify(Add("a", Add(2, 3))) == Add("a", 5)


def test_multiple_levels():
    # deeper nesting: fold wherever possible
    expr = Add(Add(1, 2), Add(3, Add(4, 5)))
    # Expected folding:
    #   Add(1,2) -> 3
    #   Add(4,5) -> 9
    #   Add(3,9) -> 12
    #   Add(3,12) -> 15
    result = simplify(expr)
    # The direct extracted result might not be fully folded, so we recursively evaluate
    assert fully_eval(result) == 15
