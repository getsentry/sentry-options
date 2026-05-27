all:
	.venv/bin/ruff check --fix
	.venv/bin/ruff format

collect-seen-options:
	python3 collect-seen-options.py 2>&1 | tee collect-seen-options-out.txt
