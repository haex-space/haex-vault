#!/usr/bin/env python3
# PreToolUse hook: nudges to check for existing helpers via graphify before writing new named units in src/** or src-tauri/src/**.
import json
import os
import re
import sys


def main():
    try:
        d = json.load(sys.stdin)
    except Exception:
        return
    t = d.get('tool_input', d)
    tool = d.get('tool_name', '')
    fp = str(t.get('file_path', '')).replace('\\', '/')
    content = str(t.get('content', '')) if tool == 'Write' else str(t.get('new_string', ''))

    cwd = os.getcwd()
    rel = fp[len(cwd) + 1:] if fp.startswith(cwd + '/') else fp

    frontend = rel.startswith('src/')
    backend = rel.startswith('src-tauri/src/')
    if not (frontend or backend):
        return

    if any(x in rel for x in ('/tests.rs', '_test.rs', '.spec.', '.test.', '__tests__/', 'src-tauri/tests/', '/fixtures/')):
        return

    if re.search(r'#\[test\]|#\[tokio::test\]|\b(it|test|describe)\s*\(', content):
        return

    ts_patterns = (
        r'\bfunction\s+\w+',
        r'\bconst\s+\w+\s*=\s*(?:\(|async|function)',
        r'\bexport\s+(?:function|class|const)',
        r'\bclass\s+\w+',
        r'defineComponent',
        r'<script\s+setup',
    )
    rs_patterns = (
        r'\bfn\s+\w+',
        r'\bpub\s+fn',
        r'\bimpl\s',
        r'\bstruct\s+\w+',
        r'\btrait\s+\w+',
        r'\benum\s+\w+',
    )
    patterns = ts_patterns if frontend else rs_patterns
    if not any(re.search(p, content) for p in patterns):
        return

    message = (
        f'Adding a named unit in {rel}. Required first step: '
        f'`graphify query "<intent>"`. If a match exists, prefer reuse or extension. '
        f'If nothing matches, or the match is genuinely a different shape, proceed. '
        f'(The hook already filtered tests and fixtures — if you are seeing this, '
        f'you are writing shared code.)'
    )
    print(json.dumps({
        'hookSpecificOutput': {
            'hookEventName': 'PreToolUse',
            'additionalContext': message,
        }
    }))


if __name__ == '__main__':
    main()
