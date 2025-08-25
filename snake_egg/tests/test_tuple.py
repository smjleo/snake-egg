from snake_egg import EGraph

def test_tuple_roundtrip():
    tup = (1, 2)
    egraph = EGraph()
    egraph.add(tup)
    assert egraph.extract(tup) == tup
