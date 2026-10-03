"""Network-free checks for corpus selection and the keyword reference baseline."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "prepare-semantic-search.py"
spec = importlib.util.spec_from_file_location("semantic_preparation", SCRIPT)
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


class CorpusPreparationTests(unittest.TestCase):
    def test_scan_excludes_generated_hidden_and_symlinked_notes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for folder in ["docs", ".git", "dist", "node_modules", "Secure"]:
                (root / folder).mkdir()
                (root / folder / "note.md").write_text("A note")
            (root / "other.txt").write_text("Not Markdown")
            (root / "linked.md").symlink_to(root / "docs/note.md")
            paths = [p.relative_to(root).as_posix() for p in prepare.markdown_paths(root)]
            self.assertEqual(paths, ["docs/note.md"])

    def test_bm25_respects_project_filter_and_deduplicates_chunks(self):
        rows = [
            {"metadata": {"text": "recover a crash", "source": "notes/recovery.md", "tags": ["notes"]}},
            {"metadata": {"text": "recover a crash again", "source": "notes/recovery.md", "tags": ["notes"]}},
            {"metadata": {"text": "recover crash crash crash", "source": "other/crash.md", "tags": ["other"]}},
            {"metadata": {"text": "install a theme", "source": "notes/theme.md", "tags": ["notes"]}},
        ]
        self.assertEqual(prepare.keyword_sources(rows, "recover crash", "notes"), ["notes/recovery.md"])
        self.assertEqual(prepare.keyword_sources(rows, "unknownword", None), [])
        self.assertEqual(prepare.keyword_sources(rows, "recover", "absent"), [])


if __name__ == "__main__":
    unittest.main()
