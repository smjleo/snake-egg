from typing import NamedTuple, Union, List, cast
from snake_egg import EGraph, Rewrite, Var, vars

class Add(NamedTuple):
    a: "Expr"
    b: "Expr"
class Eq(NamedTuple):
    a: "Expr"
    b: "Expr"
class If(NamedTuple):
    cond: "Expr"
    then: "Expr"
    els: "Expr"
class App(NamedTuple):
    f: "Expr"
    x: "Expr"
class Lam(NamedTuple):
    v: str
    body: "Expr"
class Let(NamedTuple):
    v: str
    val: "Expr"
    body: "Expr"

Expr = Union[int, bool, str, Add, Eq, If, App, Lam, Let, Var]

def replace_add(x, y):
    if isinstance(x, int) and isinstance(y, int):
        return x + y
    if isinstance(x, Add) and isinstance(x.b, int) and isinstance(y, int):
        return Add(x.a, x.b + y)
    if isinstance(y, Add) and isinstance(y.b, int) and isinstance(x, int):
        return Add(y.a, y.b + x)
    return Add(x, y)

def replace_eq(x, y):
    if isinstance(x, int) and isinstance(y, int):
        return x == y
    return Eq(x, y)

def substitute(v, val, expr):
    if isinstance(expr, str):
        return val if expr == v else expr
    if isinstance(expr, Add):
        return Add(substitute(v, val, expr.a), substitute(v, val, expr.b))
    if isinstance(expr, Eq):
        return Eq(substitute(v, val, expr.a), substitute(v, val, expr.b))
    if isinstance(expr, If):
        return If(substitute(v, val, expr.cond), substitute(v, val, expr.then), substitute(v, val, expr.els))
    if isinstance(expr, App):
        return App(substitute(v, val, expr.f), substitute(v, val, expr.x))
    if isinstance(expr, Lam):
        return expr if expr.v == v else Lam(expr.v, substitute(v, val, expr.body))
    if isinstance(expr, Let):
        if expr.v == v:
            return Let(expr.v, substitute(v, val, expr.val), expr.body)
        return Let(expr.v, substitute(v, val, expr.val), substitute(v, val, expr.body))
    return expr

def subst_rule(v, e, body):
    return substitute(v, e, body)

x, y, a, b, c, v, e, body, cond, then, els = cast(List[Var], vars("x y a b c v e body cond then els"))

rules = [
    Rewrite(Add(x, y), replace_add, name="add-fold"),
    Rewrite(Eq(x, y), replace_eq, name="eq-fold"),
    Rewrite(If(True, then, els), then, name="if-true"),
    Rewrite(If(False, then, els), els, name="if-false"),
    Rewrite(If(Eq(a, b), Add(a, a), Add(a, b)), Add(a, b), name="if-elim"),
    Rewrite(App(Lam(v, body), e), Let(v, e, body), name="beta"),
    Rewrite(Let(v, e, body), subst_rule, name="let-subst"),
]

def simplify(expr):
    egraph = EGraph()
    egraph.add(expr)
    egraph.run(rules)
    return egraph.extract(expr)

def test_lambda_if_simple():
    assert simplify(If(Eq(1, 1), 7, 9)) == 7

def test_lambda_if_elim():
    expr = If(Eq("a", "b"), Add("a", "a"), Add("a", "b"))
    assert simplify(expr) == Add("a", "b")

def test_lambda_let_simple():
    expr = Let("x", 0, Let("y", 1, Add("x", "y")))
    assert simplify(expr) == 1

def test_lambda_compose():
    compose = Lam("f", Lam("g", Lam("x", App("f", App("g", "x")))))
    add1 = Lam("y", Add("y", 1))
    expr = Let("compose", compose, Let("add1", add1, App(App("compose", "add1"), "add1")))
    assert simplify(expr) == Lam("x", Add("x", 2))

def test_lambda_compose_many():
    compose = Lam("f", Lam("g", Lam("x", App("f", App("g", "x")))))
    add1 = Lam("y", Add("y", 1))
    def compose_n(n, expr):
        for _ in range(n):
            expr = App(App("compose", "add1"), expr)
        return expr
    expr = Let("compose", compose, Let("add1", add1, compose_n(6, "add1")))
    assert simplify(expr) == Lam("x", Add("x", 7))
