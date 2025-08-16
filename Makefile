.ONESHELL:
SHELL = /bin/zsh

.PHONY: all build test install doc clean distclean

activate:
	source $$(conda info --base)/etc/profile.d/conda.sh && conda activate detective

all: test

venv: activate
	python -m pip install maturin==0.14

build: venv
	maturin build --release

test: snake_egg/tests/*.py build venv
	maturin develop && python snake_egg/tests/test_math.py
	maturin develop && python snake_egg/tests/test_prop.py
	maturin develop && python snake_egg/tests/test_simple.py
	maturin develop && python snake_egg/tests/test_dynamic.py
	maturin develop && python snake_egg/tests/test_dataclass.py

stubtest: snake_egg/__init__.pyi build venv
	maturin develop --extras=dev && python -m mypy.stubtest snake_egg --ignore-missing-stub

mypy: snake_egg/__init__.pyi build venv
	maturin develop --extras=dev && mypy snake_egg

install: venv
	maturin build --release && \
	  python -m pip install snake_egg --force-reinstall --no-index \
	  --find-link ./target/wheels/

doc: venv
	maturin develop && python -m pydoc -w snake_egg.internal

shell: venv
	maturin develop && python -ic 'import snake_egg'

clean:
	cargo clean

distclean: clean
	$(RM) -r venv
