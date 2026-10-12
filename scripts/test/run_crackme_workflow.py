#!/usr/bin/env python3
"""Validate a supplied crackme candidate using Fission evidence and an optional oracle.

This is a workflow gate, not an automatic solver. Candidate and success text
must come from prior binary analysis; no target identity is built into Fission.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--candidate', required=True)
    parser.add_argument('--success-text', required=True)
    parser.add_argument('--reject', action='append', required=True,
                        help='Known nonmatching inputs; repeat for boundary cases')
    parser.add_argument('--artifacts', type=Path, required=True)
    parser.add_argument('--oracle-image', help='Optional Linux/amd64 Python Docker image')
    args = parser.parse_args()
    cli, binary, artifacts = args.cli.resolve(), args.binary.resolve(), args.artifacts.resolve()
    artifacts.mkdir(parents=True, exist_ok=True)
    original = binary.read_bytes()
    digest = hashlib.sha256(original).hexdigest()
    records = []

    def execute(label, command):
        result = subprocess.run(command, capture_output=True, timeout=60)
        (artifacts / (label + '.stdout')).write_bytes(result.stdout)
        (artifacts / (label + '.stderr')).write_bytes(result.stderr)
        records.append({'label': label, 'command': command, 'returncode': result.returncode})
        if result.returncode:
            raise RuntimeError(f'{label} failed: {result.stderr.decode(errors="replace")}')
        return json.loads(result.stdout)

    def fission(label, command, options):
        return execute(label, [str(cli), '--no-db', command, str(binary), *options, '--json'])

    info = fission('info', 'info', [])
    leads = fission('strings', 'strings', ['--contains', args.success_text, '--context'])
    functions = sorted({function['address'] for row in leads['strings']
                        for function in (row.get('context') or {}).get('functions', [])})
    if not functions:
        raise RuntimeError('No known function owns this success string; inspect reference evidence manually')
    for entry in functions:
        fission('callgraph-' + entry, 'callgraph', ['--function', entry])
        fission('disasm-' + entry, 'disasm', ['--addr', entry, '--function'])

    oracle_script = '''import json,pathlib,subprocess,sys
p=pathlib.Path('/tmp/crackme')
p.write_bytes(pathlib.Path('/input/sample').read_bytes()); p.chmod(0o500)
r=subprocess.run([str(p),sys.argv[1]],capture_output=True,timeout=5)
print(json.dumps(dict(stdout=list(r.stdout),stderr=list(r.stderr),exit_code=r.returncode)))'''
    verdicts = []
    for number, candidate in enumerate([args.candidate, *args.reject]):
        label = f'input-{number}'
        report = execute(label + '-sandbox', [str(cli), '--no-db', 'sandbox', str(binary),
            '--json', '--max-inst', '10000', '--fail-on-budget', '--max-hle-misses', '0',
            '--max-unknown-syscalls', '0', '--', candidate])
        output = report['guest_output']
        if output['stdout'] is None or output['stderr'] is None:
            raise RuntimeError('Guest console capture incomplete')
        if not report['halt_requested'] or report.get('exit_code') is None:
            raise RuntimeError('Instruction bound reached without a guest exit')
        for key in ['unimplemented_opcodes', 'hle_misses', 'unknown_syscalls', 'unhandled_userops']:
            if any(report['metrics'][key].values()):
                raise RuntimeError(f'Unsupported execution semantics: {key}')
        success = args.success_text.encode() in bytes(output['stdout'])
        if success != (number == 0):
            raise RuntimeError(f'{label}: candidate/rejection verdict mismatch')
        if args.oracle_image:
            oracle = execute(label + '-oracle', ['docker', 'run', '--rm', '--platform', 'linux/amd64',
                '--network', 'none', '--read-only', '--cap-drop', 'ALL', '--security-opt',
                'no-new-privileges', '--user', '65534:65534', '--memory', '128m', '--pids-limit', '32',
                '--tmpfs', '/tmp:rw,exec,size=16m', '-v', f'{binary}:/input/sample:ro',
                args.oracle_image, 'python', '-c', oracle_script, candidate])
            # Native signals use negative subprocess return codes; shell-style
            # signal status alone does not prove signal-delivery equivalence.
            if oracle['exit_code'] < 0:
                raise RuntimeError('Native oracle terminated by signal; compare manually')
            for stream in ['stdout', 'stderr']:
                if oracle[stream] != output[stream]:
                    raise RuntimeError(f'{label}: native {stream} differs')
            if oracle['exit_code'] != report['exit_code']:
                raise RuntimeError(f'{label}: native exit status differs')
        verdicts.append({'input_index': number, 'accepted': success,
                         'exit_code': report['exit_code'], 'instructions': report['metrics']['instructions']})
    if binary.read_bytes() != original:
        raise RuntimeError('Analysis mutated the input binary')
    summary = {'binary_sha256': digest, 'cli_sha256': hashlib.sha256(cli.read_bytes()).hexdigest(),
               'info': info, 'validator_entries': functions, 'verdicts': verdicts,
               'native_oracle': args.oracle_image, 'commands': records,
               'scope': 'supplied-candidate validation, not automatic solving or runtime completeness'}
    (artifacts / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
