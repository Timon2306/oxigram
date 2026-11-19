"""Builds translations/<lang>/LC_MESSAGES/oxigram-native.po from the header
already there and the parts in translations/parts/*.<lang>.po (used while
strings are being moved to English sources; the first entry of an msgid wins).

    python native/translations/merge.py
"""
import glob
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))


def entries(text):
    """Splits a .po text into (msgid, block) pairs, skipping the header."""
    blocks = re.split(r'\n\s*\n', text.strip())
    out = []
    for block in blocks:
        m = re.search(r'^msgid "(.*)"((?:\n".*")*)', block, re.M)
        if not m:
            continue
        msgid = m.group(1) + ''.join(re.findall(r'^"(.*)"$', m.group(2), re.M))
        if msgid == '':
            continue
        out.append((msgid, block.strip()))
    return out


for lang in ('ru', 'uk'):
    target = os.path.join(HERE, lang, 'LC_MESSAGES', 'oxigram-native.po')
    text = open(target, encoding='utf-8').read()
    header = text.split('\n\n')[0].strip()
    seen = {}
    for msgid, block in entries(text):
        seen.setdefault(msgid, block)
    for part in sorted(glob.glob(os.path.join(HERE, 'parts', f'*.{lang}.po'))):
        for msgid, block in entries(open(part, encoding='utf-8').read()):
            seen.setdefault(msgid, block)
    with open(target, 'w', encoding='utf-8', newline='\n') as f:
        f.write(header + '\n\n' + '\n\n'.join(seen.values()) + '\n')
    print(lang, len(seen), 'entries')
