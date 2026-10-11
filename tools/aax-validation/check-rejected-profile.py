#!/usr/bin/env python3
"""Require supported raw input compilation and exact raw+generated refusal."""
from pathlib import Path
import argparse,subprocess
parser=argparse.ArgumentParser();parser.add_argument('--compiler',required=True);parser.add_argument('--native-include',type=Path,required=True);parser.add_argument('--aax-sdk',type=Path,required=True);args=parser.parse_args()
source=Path(__file__).with_name('reject-raw-generated.cpp')
command=[args.compiler,'-std=c++17','-fsyntax-only',f'-I{args.native_include}',f'-I{args.aax_sdk/"Interfaces"}',f'-I{args.aax_sdk/"Interfaces/ACF"}',str(source)]
for bits in (32,64):
 for mode in ([],['-ffast-math']):
    flags=[*mode,f'-DRAVET_TEST_SAMPLE_BITS={bits}']
    positive=subprocess.run([*command,*flags,'-DRAVET_TEST_OUTPUT_CAPACITY=0'],capture_output=True,text=True)
    if positive.returncode:raise SystemExit('supported raw input compile failed: '+positive.stderr)
    negative=subprocess.run([*command,*flags],capture_output=True,text=True)
    if not negative.returncode or 'Raw short MIDI input selects the input-only Context' not in negative.stderr:raise SystemExit('missing exact raw/generated compile refusal: '+negative.stderr)
print('PASS supported raw input and exact raw+generated processor/model refusal,32/64 normal/fast')
