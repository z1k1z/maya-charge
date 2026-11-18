#!/usr/bin/env python3
"""Preserve comments/formatting and verify all unrelated TOML values stay intact."""
import argparse
import copy
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
import tomllib

WIDGET = 'mouse_charge'
TYPE = 'local/mouse-charge:battery'


def patch(text, *, state=False, remove=False):
    original = tomllib.loads(text)
    expected = copy.deepcopy(original)
    bar = original.get('bar', {}).get('default', {})
    if not state and 'start' not in bar:
        raise ValueError('Не найдена bar.default.start; конфигурация не изменена')
    if 'start' in bar:
        items = [item for item in bar['start'] if item != WIDGET]
        if not remove:
            if 'group:g1' not in items:
                raise ValueError('Не найдена группа нагрузки group:g1; конфигурация не изменена')
            items.insert(items.index('group:g1') + 1, WIDGET)
        expected['bar']['default']['start'] = items
        section = re.search(r'(?m)^\[bar\.default\]\s*\n(?:(?!\s*\[).*(?:\n|$))*', text)
        if not section:
            raise ValueError('Неподдерживаемый формат секции bar.default')
        body, count = re.subn(r'(?ms)^start\s*=\s*\[.*?\]', lambda _: 'start = ' + json.dumps(items), section.group())
        if count != 1:
            raise ValueError('Не удалось однозначно изменить start')
        text = text[:section.start()] + body + text[section.end():]
    if not state:
        existing = original.get('widget', {}).get(WIDGET)
        if existing and existing.get('type') != TYPE:
            raise ValueError('Имя mouse_charge уже занято другим виджетом')
        if remove and existing is not None:
            pattern = r'(?ms)^\[widget\.mouse_charge\]\n.*?(?=^\[|\Z)'
            text, count = re.subn(pattern, '', text)
            if count != 1:
                raise ValueError('Не удалось удалить секцию виджета')
            del expected['widget'][WIDGET]
        elif not remove and existing is None:
            text = text.rstrip() + '\n\n[widget.mouse_charge]\ntype = "' + TYPE + '"\n'
            expected.setdefault('widget', {})[WIDGET] = {'type': TYPE}
    if tomllib.loads(text) != expected:
        raise ValueError('Проверка сохранности остальных настроек не пройдена')
    return text


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--apply', action='store_true')
    parser.add_argument('--remove', action='store_true')
    parser.add_argument('--output-dir', type=Path, default=Path('target/noctalia-preview'))
    args = parser.parse_args()
    config_dir = Path(os.environ.get('NOCTALIA_CONFIG_HOME') or Path(os.environ.get('XDG_CONFIG_HOME', Path.home()/'.config'))/'noctalia')
    state_dir = Path(os.environ.get('NOCTALIA_STATE_HOME') or Path(os.environ.get('XDG_STATE_HOME', Path.home()/'.local/state'))/'noctalia')
    paths = [(config_dir/'config.toml', False), (state_dir/'settings.toml', True)]
    args.output_dir.mkdir(parents=True, exist_ok=True)
    changes = []
    for path, is_state in paths:
        if is_state and not path.exists():
            continue
        text = path.read_text()
        modified = patch(text, state=is_state, remove=args.remove)
        preview = args.output_dir/('settings.toml' if is_state else 'config.toml')
        preview.write_text(modified)
        if text != modified:
            subprocess.run(['noctalia', 'config', 'validate', str(preview)], check=True)
            changes.append((path, text, modified))
    if not args.apply:
        print(f'Предпросмотр: {args.output_dir.resolve()}')
        return
    # Validate everything before changing any config. Backups remain beside files.
    for path, old, new in changes:
        if path.read_text() != old:
            raise RuntimeError(f'{path} изменён во время установки; повтори запуск')
        backup = path.with_name(path.name + '.mouse-charge-' + str(time.time_ns()) + '.bak')
        shutil.copy2(path, backup)
        fd, temporary = tempfile.mkstemp(prefix='.mouse-charge-', dir=path.parent)
        try:
            with os.fdopen(fd, 'w') as stream:
                stream.write(new)
            os.chmod(temporary, path.stat().st_mode & 0o777)
            os.replace(temporary, path)
        finally:
            if os.path.exists(temporary):
                os.unlink(temporary)
        print(f'Изменён {path}; резервная копия: {backup}')


if __name__ == '__main__':
    main()
