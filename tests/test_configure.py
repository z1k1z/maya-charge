import importlib.util
from pathlib import Path
import tomllib
import unittest

spec = importlib.util.spec_from_file_location('configure', Path(__file__).parents[1]/'scripts/configure.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

CONFIG = '''# Keep my theme
[bar.default]
start = [ "launcher", "group:g1", "clock" ]
thickness = 35

    [[bar.default.capsule_group]]
    id = "g1"
    members = ["temp", "sysmon_3"]

[widget.clock]
color = "secondary"
'''

class ConfigTests(unittest.TestCase):
    def test_install_idempotent_and_remove(self):
        installed = module.patch(CONFIG)
        self.assertIn('# Keep my theme', installed)
        self.assertEqual(tomllib.loads(installed)['bar']['default']['start'], ['launcher','group:g1','mouse_charge','clock'])
        self.assertEqual(module.patch(installed), installed)
        self.assertEqual(tomllib.loads(module.patch(installed, remove=True)), tomllib.loads(CONFIG))

    def test_multiline_and_state_override(self):
        text = CONFIG.replace('[ "launcher", "group:g1", "clock" ]', '[\n "launcher",\n "group:g1",\n "clock"\n]')
        installed = module.patch(text, state=True)
        self.assertNotIn('[widget.mouse_charge]', installed)
        self.assertIn('mouse_charge', tomllib.loads(installed)['bar']['default']['start'])
        self.assertEqual(module.patch('[theme]\nmode="dark"\n', state=True), '[theme]\nmode="dark"\n')

    def test_unknown_layout_fails_without_overwriting(self):
        with self.assertRaises(ValueError):
            module.patch(CONFIG.replace('group:g1', 'group:other'))
        with self.assertRaises(ValueError):
            module.patch(CONFIG+'\n[widget.mouse_charge]\ntype="clock"\n')

if __name__ == '__main__':
    unittest.main()
