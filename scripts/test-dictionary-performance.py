#!/usr/bin/env python3
"""Compare smart OCR scans with a frozen pre-optimization oracle on temporary SQLite.

Use --benchmark for interleaved timings; correctness uses payloads and SELECT
counts, never wall-clock thresholds. No user dictionaries or network are used.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path
import random
import re
import sqlite3
import statistics
import tempfile
import time
from typing import Any
import unittest

TOOL = Path(__file__).resolve().with_name("japanese-dictionary.py")
spec = importlib.util.spec_from_file_location("japanese_dictionary", TOOL)
dictionary = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dictionary)
_smart_surface_match = dictionary._smart_surface_match
_metadata_for = dictionary._metadata_for
kanji_lookup = dictionary.kanji_lookup
romanize_kana = dictionary.romanize_kana


# Frozen original traversal/ranking/hydration; shared unchanged lookup helpers
# keep this oracle focused on the smart_scan optimization.
def baseline_smart_scan(db: sqlite3.Connection, text: str, limit: int, max_chars: int) -> dict[str, Any]:
    # OCR often includes bullets, romaji and an English translation around the
    # Japanese phrase. Scan Japanese runs rather than only shrinking a prefix
    # from byte/character zero; otherwise a valid one-kana entry such as ま can
    # win simply because noise precedes the actual phrase (e.g. またね).
    # Tesseract can insert spaces between Japanese glyphs (ま た ね). Japanese
    # prose normally has no word-separating spaces, so collapse horizontal
    # whitespace only when both neighbours are Japanese. Keep newlines intact
    # so separate OCR lines remain separate candidate runs.
    jp_char = r"ぁ-ゖァ-ヺー一-龯々〆ヵヶ"
    normalized = re.sub(rf"(?<=[{jp_char}])[ \t]+(?=[{jp_char}])", "", text)
    japanese_runs = re.findall(rf"[{jp_char}]+", normalized)
    if not japanese_runs:
        fallback = re.sub(r"^[\s\u3000、。！？!?「」『』（）()【】]+", "", text.strip())
        japanese_runs = [fallback] if fallback else []

    best: tuple[tuple[int, int, float, int, int], dict[str, Any]] | None = None
    for run_index, raw_run in enumerate(japanese_runs):
        run = raw_run[:max_chars]
        for start in range(len(run)):
            tail = run[start:]
            for size in range(len(tail), 0, -1):
                surface = tail[:size]
                match = _smart_surface_match(db, surface, limit)
                if match is None:
                    continue
                base, reason, accepted = match
                top_score = max(float(term.get("score") or 0) for term in accepted)
                # Multi-character expressions are overwhelmingly more useful
                # for OCR lookup than isolated kana. Length outranks dictionary
                # frequency; score and earlier position break ties.
                rank = (1 if len(surface) > 1 else 0, -run_index, -start, len(surface), top_score)
                reading = accepted[0].get("reading") or base
                payload = {
                    "query": text, "surface": surface, "matched": base, "consumed": len(surface),
                    "deinflection": reason, "reading": reading, "romaji": romanize_kana(reading),
                    "terms": accepted, "metadata": _metadata_for(db, base),
                    "kanji": kanji_lookup(db, base).get("kanji", []),
                    "segment": run, "segmentOffset": start,
                }
                if best is None or rank > best[0]:
                    best = (rank, payload)
                break

    if best is not None:
        return best[1]
    return {"query": text, "surface": "", "matched": "", "consumed": 0, "deinflection": "", "reading": "", "romaji": "",
            "terms": [], "metadata": {"pitch": [], "frequency": [], "ipa": []}, "kanji": []}


def populate(db: sqlite3.Connection, *, rich: bool = True) -> None:
    db.executemany(
        "INSERT INTO dictionaries(id, title, imported_at) VALUES (?, ?, ?)",
        [(1, "Fixture low", "2026-01-01"), (2, "Fixture high", "2026-01-01")],
    )
    entries = [(word, word, "", 0, 1) for word in ["日本", "本", "語", "学", "校", "世", "界"]]
    if rich:
        entries += [
            ("ま", "ま", "", 999, 1), ("またね", "またね", "", 1, 1),
            ("日本語", "にほんご", "", -1, 1), ("学校", "がっこう", "", 50, 1),
            ("世界", "せかい", "", 100, 1), ("日本", "にほん", "", 50, 2),
            ("日本", "にっぽん", "", 50, 2),
            ("幾ら", "いくら", "", 20, 2), ("いくら", "いくら", "", 1, 1),
            ("食べる", "たべる", "v1", 20, 2), ("食べる", "たべる", "n", 900, 1),
            ("たべる", "たべる", "v1", 1, 1), ("書く", "かく", "v5", 10, 1),
            ("する", "する", "vs", 10, 1), ("来る", "くる", "vk", 10, 1),
            ("高い", "たかい", "adj-i", 10, 1),
            ("hello", "hello", "", 5, 1), ("abc", "abc", "", 10, 1),
            ("a", "a", "", 900, 1), ("漢", "かん", "", 10, 1),
        ]
    db.executemany(
        """INSERT INTO terms(dictionary_id, expression, reading, definition_tags,
           rules, score, glossary_json, sequence, term_tags)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)""",
        [(dictionary_id, word, reading, "common noun", rules, score,
          json.dumps([f"definition {word}", {"type": "structured-content", "content": {
              "data": {"content": "glossary"}, "content": ["structured", word]}}], ensure_ascii=False),
          index if index % 2 else -1, "fixture tag")
         for index, (word, reading, rules, score, dictionary_id) in enumerate(entries)],
    )
    for word in dict.fromkeys(row[0] for row in entries):
        db.executemany(
            "INSERT INTO term_meta(dictionary_id, expression, meta_type, data_json) VALUES (?, ?, ?, ?)",
            [(1, word, "freq", json.dumps({"reading": word, "frequency": 17})),
             (2, word, "pitch", json.dumps({"reading": word, "pitches": [{"position": 1}]})),
             (1, word, "ipa", "invalid JSON")],
        )
    for character in dict.fromkeys("日本語学校世界食書来高漢"):
        db.executemany(
            """INSERT INTO kanji(dictionary_id, character, onyomi, kunyomi, tags,
               meanings_json, stats_json) VALUES (?, ?, ?, ?, ?, ?, ?)""",
            [(1, character, "オン ヨミ", "くん よみ", "common fixture",
              json.dumps([character, "meaning"]), json.dumps({"strokes": "4"})),
             (2, character, "オン", "", "", "invalid JSON", "invalid JSON")],
        )
    db.commit()


def counted_scan(scan, db, text, limit=16, max_chars=32, *, selects=None):
    if selects is None:
        selects = []
    db.set_trace_callback(lambda sql: selects.append(sql) if sql.lstrip().upper().startswith("SELECT") else None)
    try:
        result = scan(db, text, limit, max_chars)
    finally:
        db.set_trace_callback(None)
    return result, len(selects)


class SmartScanTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="intoo-dictionary-tests-")
        self.addCleanup(temporary.cleanup)
        self.db = dictionary.connect(Path(temporary.name) / "fixture.sqlite3")
        self.addCleanup(self.db.close)
        populate(self.db)

    def compare(self, text, limit=16, max_chars=32):
        expected, before = counted_scan(baseline_smart_scan, self.db, text, limit, max_chars)
        actual, after = counted_scan(dictionary.smart_scan, self.db, text, limit, max_chars)
        self.assertEqual(actual, expected)
        # Preserve the emitted JSON field order as well as the nested payload.
        self.assertEqual(json.dumps(actual, ensure_ascii=False), json.dumps(expected, ensure_ascii=False))
        self.assertLessEqual(after, before)
        return actual, before, after

    def test_ranking_prefers_multi_then_earlier_run_offset_then_longer(self):
        cases = [
            ("ま\n世界", "世界", "世界", 0),
            ("ま世界", "世界", "ま世界", 1),
            ("日本\n世界", "日本", "日本", 0),
            ("日本学校", "日本", "日本学校", 0),
            ("本語学校", "学校", "本語学校", 2),
            ("日本語学校", "日本語", "日本語学校", 0),
            ("ま\n漢", "ま", "ま", 0),
        ]
        for text, surface, segment, offset in cases:
            with self.subTest(text=text):
                result, _, _ = self.compare(text)
                self.assertEqual((result["surface"], result["segment"], result["segmentOffset"]),
                                 (surface, segment, offset))

    def test_reading_deinflection_rules_and_dictionary_order(self):
        cases = [("食べました", "食べる", "polite past", "食べる"),
                 ("たべました", "たべる", "polite past", "食べる"),
                 ("書いた", "書く", "past/te-form", "書く"),
                 ("しました", "する", "irregular", "する"),
                 ("きました", "来る", "irregular", "来る"),
                 ("高かった", "高い", "past", "高い"),
                 ("いくら", "いくら", "", "幾ら")]
        for text, base, reason, expression in cases:
            for limit in [1, 2, 16]:
                with self.subTest(text=text, limit=limit):
                    result, _, _ = self.compare(text, limit)
                    if limit == 16:
                        self.assertEqual((result["matched"], result["deinflection"], result["terms"][0]["expression"]),
                                         (base, reason, expression))
                        if reason == "polite past":
                            self.assertTrue(all("v1" in term["rules"] for term in result["terms"]))
        result, _, _ = self.compare("日本")
        self.assertEqual([term["reading"] for term in result["terms"]], ["にほん", "にっぽん", "日本"])

    def test_whitespace_newlines_fallback_and_per_run_truncation(self):
        cases = [("• translation: 日 本\t語!", 32, "日本語"),
                 ("日\n本", 32, "本"), ("ま\n日本語学校", 3, "日本語"),
                 ("日本語学校", 2, "日本"), ("日本語学校", 1, ""),
                 ("ま\n世界", 1, "ま"), ("   「hello world", 32, "hello"),
                 ("abc hello", 32, "abc"), ("a hello", 32, "hello"),
                 ("", 32, ""), ("!?　", 32, ""), ("未知", 32, ""),
                 ("日本", 0, ""), ("日本語", -1, "日本")]
        for text, max_chars, surface in cases:
            with self.subTest(text=text, max_chars=max_chars):
                result, _, _ = self.compare(text, max_chars=max_chars)
                self.assertEqual(result["surface"], surface)
                if not surface:
                    self.assertNotIn("segment", result)
                    self.assertEqual(result["metadata"], {"pitch": [], "frequency": [], "ipa": []})

    def test_winner_metadata_and_kanji_are_exact(self):
        result, before, after = self.compare("ま日本\n世界")
        self.assertEqual(result["matched"], "日本")
        self.assertEqual(result["metadata"]["frequency"], [
            {"dictionary": "Fixture low", "data": {"reading": "日本", "frequency": 17}}])
        self.assertEqual(result["metadata"]["ipa"], [{"dictionary": "Fixture low", "data": None}])
        self.assertEqual([entry["character"] for entry in result["kanji"]], ["日", "日", "本", "本"])
        self.assertEqual(result["kanji"][1]["meanings"], [])
        self.assertLess(after, before)

    def test_single_winner_hydrates_only_once(self):
        result, before, after = self.compare("ま" * 12)
        self.assertEqual(result["surface"], "ま")
        self.assertLess(after, before)
        selects = []
        counted_scan(dictionary.smart_scan, self.db, "ま" * 12, selects=selects)
        self.assertEqual(sum("FROM term_meta" in sql for sql in selects), 1)
        self.assertEqual(sum("FROM kanji" in sql for sql in selects), 1)

    def test_randomized_differential_scan(self):
        rng = random.Random(90421)
        pieces = ["日本", "日本語", "学校", "世界", "ま", "またね", "食べました", "たべました",
                  "書いて", "高くない", "いくら", "きた", "漢", "未知", "hello", "abc", "a",
                  " ", "\t", "\n", "!?", "日 本 語"]
        for trial in range(250):
            text = "".join(rng.choice(pieces) for _ in range(rng.randrange(0, 7)))
            limit = rng.choice([0, 1, 2, 4, 16, -1])
            max_chars = rng.choice([-2, 0, 1, 2, 3, 7, 16, 32])
            with self.subTest(trial=trial, text=text, limit=limit, max_chars=max_chars):
                self.compare(text, limit, max_chars)

    def test_representative_select_count(self):
        # Match the measured sparse dictionary rather than the richer fixture.
        self.db.execute("DELETE FROM terms")
        self.db.execute("DELETE FROM term_meta")
        self.db.execute("DELETE FROM kanji")
        self.db.execute("DELETE FROM dictionaries")
        populate(self.db, rich=False)
        result, before, after = self.compare("日本語学校世界" * 3)
        self.assertEqual(result["surface"], "日本")
        self.assertEqual((before, after), (273, 23))
        print(f"smart_scan representative SELECTs: {before} -> {after}")

    def test_randomized_dictionary_rows_and_scans(self):
        rng = random.Random(48216)
        for fixture in range(6):
            self.db.execute("DELETE FROM terms")
            words = ["".join(rng.choice("日本語学校またね") for _ in range(rng.randrange(1, 5)))
                     for _ in range(25)]
            self.db.executemany(
                """INSERT INTO terms(dictionary_id, expression, reading, rules, score, glossary_json)
                   VALUES (?, ?, ?, ?, ?, ?)""",
                [(rng.choice([1, 2]), word, rng.choice(["", "かな", "またね", "にほん"]),
                  rng.choice(["", "n", "v1", "v5"]), rng.randrange(-20, 21), json.dumps([word]))
                 for word in words],
            )
            self.db.commit()
            for trial in range(40):
                text = rng.choice(["", "ま", "• "]) + rng.choice(words + ["かな", "にほん", "未知"])
                text += rng.choice(["", "\n", "!", " \t"]) + rng.choice(words)
                limit, max_chars = rng.choice([1, 2, 16]), rng.choice([1, 3, 8, 16])
                with self.subTest(fixture=fixture, trial=trial, text=text):
                    self.compare(text, limit, max_chars)


def benchmark(runs):
    with tempfile.TemporaryDirectory(prefix="intoo-dictionary-benchmark-") as temporary:
        db = dictionary.connect(Path(temporary) / "fixture.sqlite3")
        try:
            populate(db, rich=False)
            text = "日本語学校世界" * 3
            scans = [("before", baseline_smart_scan), ("after", dictionary.smart_scan)]
            timings = {name: [] for name, _ in scans}
            counts = {}
            for name, scan in scans:
                _, counts[name] = counted_scan(scan, db, text)
                scan(db, text, 16, 32)
            for iteration in range(runs):
                for name, scan in scans[::1 if iteration % 2 == 0 else -1]:
                    started = time.perf_counter()
                    scan(db, text, 16, 32)
                    timings[name].append(time.perf_counter() - started)
            print(f"Interleaved warm SQLite benchmark ({runs} runs per version):")
            for name, _ in scans:
                print(f"  {name}: SELECTs={counts[name]}, mean={statistics.mean(timings[name]):.6f}s, "
                      f"median={statistics.median(timings[name]):.6f}s")
        finally:
            db.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--benchmark", action="store_true")
    parser.add_argument("--runs", type=int, default=20)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(SmartScanTests)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if not result.wasSuccessful():
        raise SystemExit(1)
    if args.benchmark:
        benchmark(args.runs)
