#!/usr/bin/env python3
import os
import shutil
import sys
import time
from pathlib import Path

arguments = sys.argv[1:]
input_index = arguments.index("--input")
output_index = arguments.index("--output")
input_path = Path(arguments[input_index + 1])
output_path = Path(arguments[output_index + 1])
root = Path(__file__).resolve().parent

shutil.copyfile(root / "synthetic-contract.raw.glb", output_path)
if os.fork() == 0:
    os.setsid()
    time.sleep(3)
    os._exit(0)
sys.exit(0)
