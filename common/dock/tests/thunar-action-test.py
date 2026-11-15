#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("thunar_action", Path(__file__).resolve().parents[1] / "tools/thunar_action.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class ActionsTest(unittest.TestCase):
    def test_existing_actions_idempotence_and_removal(self):
        with tempfile.TemporaryDirectory() as temp:
            config = Path(temp) / "uca.xml"
            config.write_text('<actions><!-- keep --><action><name>My action</name><unique-id>mine</unique-id><command>echo unchanged</command></action></actions>')
            self.assertTrue(module.update(config, Path("/home/a name/.local/bin/dock")))
            root = ET.parse(config).getroot()
            self.assertEqual(root.find("action/command").text, "echo unchanged")
            self.assertEqual(len(root.findall("action")), 2)
            self.assertIn("keep", config.read_text())
            self.assertEqual(len(list(config.parent.glob("uca.xml.dock-backup-*"))), 1)
            self.assertFalse(module.update(config, Path("/home/a name/.local/bin/dock")))
            self.assertEqual(len(list(config.parent.glob("uca.xml.dock-backup-*"))), 1)
            self.assertTrue(module.update(config, None))
            self.assertEqual(len(ET.parse(config).getroot().findall("action")), 1)
            self.assertFalse(module.update(config, None))
    def test_thunar_reversed_xml_declaration(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "uca.xml"
            original = b'<?xml encoding="UTF-8" version="1.0"?>\n<actions><action><name>Run command ...</name><unique-id>mine</unique-id></action></actions>'
            path.write_bytes(original)
            self.assertTrue(module.update(path, Path("/bin/dock")))
            root = ET.parse(path).getroot()
            self.assertEqual(root.find("action/name").text, "Run command ...")
            self.assertEqual(len(root.findall("action")), 2)
            backup = next(path.parent.glob("uca.xml.dock-backup-*"))
            self.assertEqual(backup.read_bytes(), original)
            self.assertFalse(module.update(path, Path("/bin/dock")))

    def test_symlink_is_preserved(self):
        with tempfile.TemporaryDirectory() as temp:
            target = Path(temp) / "real.xml"; target.write_text("<actions />")
            path = Path(temp) / "uca.xml"; path.symlink_to(target)
            self.assertTrue(module.update(path, Path("/bin/dock")))
            self.assertTrue(path.is_symlink())
            self.assertEqual(len(ET.parse(target).getroot().findall("action")), 1)

    def test_malformed_xml_is_not_overwritten(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "uca.xml"; path.write_text("broken XML")
            with self.assertRaises(ET.ParseError): module.update(path, Path("/bin/dock"))
            self.assertEqual(path.read_text(), "broken XML")
    def test_remove_without_existing_config_creates_nothing(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "missing/uca.xml"
            self.assertFalse(module.update(path, None)); self.assertFalse(path.parent.exists())

if __name__ == '__main__': unittest.main()
