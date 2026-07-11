@echo off
rem Entry launcher: the host runs an executable, so this .cmd starts Python on plugin.py.
rem Uses the `py` launcher (ships with the python.org installer). If you only have
rem `python` on PATH, change the line below to: python "%~dp0plugin.py"
py -3 "%~dp0plugin.py"
