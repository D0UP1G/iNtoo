#!/usr/bin/env node
// Regression and optional benchmark for exact UTF-16 fuzzy-match scores.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { performance } = require("node:perf_hooks");

const source = fs.readFileSync(path.join(__dirname, "../modules/common/functions/levendist.js"), "utf8");
function load(code) {
    const context = {};
    vm.runInNewContext(code, context);
    return context;
}
const actual = load(source);

// Frozen pre-optimization algorithm; retained separately from the production
// source so regression expectations and benchmark timings cannot drift with it.
const baselineSource = String.raw`// Original code from https://github.com/koeqaife/hyprland-material-you
// Original code license: GPLv3
// Translated to Js from Cython with an LLM and reviewed

function min3(a, b, c) {
    return a < b && a < c ? a : b < c ? b : c;
}

function max3(a, b, c) {
    return a > b && a > c ? a : b > c ? b : c;
}

function min2(a, b) {
    return a < b ? a : b;
}

function max2(a, b) {
    return a > b ? a : b;
}

function levenshteinDistance(s1, s2, rows) {
    let len1 = s1.length;
    let len2 = s2.length;

    if (len1 === 0) return len2;
    if (len2 === 0) return len1;

    if (len2 > len1) {
        [s1, s2] = [s2, s1];
        [len1, len2] = [len2, len1];
    }

    let prev = rows ? rows[0] : new Array(len2 + 1);
    let curr = rows ? rows[1] : new Array(len2 + 1);

    for (let j = 0; j <= len2; j++) {
        prev[j] = j;
    }

    for (let i = 1; i <= len1; i++) {
        curr[0] = i;
        for (let j = 1; j <= len2; j++) {
            let cost = s1[i - 1] === s2[j - 1] ? 0 : 1;
            curr[j] = min3(prev[j] + 1, curr[j - 1] + 1, prev[j - 1] + cost);
        }
        const tmp = prev;
        prev = curr;
        curr = tmp;
    }

    return prev[len2];
}

function partialRatio(shortS, longS) {
    let lenS = shortS.length;
    let lenL = longS.length;
    let best = 0.0;

    if (lenS === 0) return 1.0;
    if (longS.includes(shortS)) return 1.0;

    // Every sliding window has the same size; reuse its distance workspace.
    const rows = [new Array(lenS + 1), new Array(lenS + 1)];

    for (let i = 0; i <= lenL - lenS; i++) {
        let sub = longS.slice(i, i + lenS);
        let dist = levenshteinDistance(shortS, sub, rows);
        let score = 1.0 - (dist / lenS);
        if (score > best) best = score;
    }

    return best;
}

function computeScore(s1, s2) {
    if (s1 === s2) return 1.0;

    let dist = levenshteinDistance(s1, s2);
    let maxLen = max2(s1.length, s2.length);
    if (maxLen === 0) return 1.0;

    let full = 1.0 - (dist / maxLen);
    let part = s1.length < s2.length ? partialRatio(s1, s2) : partialRatio(s2, s1);

    let score = 0.85 * full + 0.15 * part;

    if (s1 && s2 && s1[0] !== s2[0]) {
        score -= 0.05;
    }

    let lenDiff = Math.abs(s1.length - s2.length);
    if (lenDiff >= 3) {
        score -= 0.05 * lenDiff / maxLen;
    }

    let commonPrefixLen = 0;
    let minLen = min2(s1.length, s2.length);
    for (let i = 0; i < minLen; i++) {
        if (s1[i] === s2[i]) {
            commonPrefixLen++;
        } else {
            break;
        }
    }
    score += 0.02 * commonPrefixLen;

    if (s1.includes(s2) || s2.includes(s1)) {
        score += 0.06;
    }

    return Math.max(0.0, Math.min(1.0, score));
}

function computeTextMatchScore(s1, s2) {
    if (s1 === s2) return 1.0;

    let dist = levenshteinDistance(s1, s2);
    let maxLen = max2(s1.length, s2.length);
    if (maxLen === 0) return 1.0;

    let full = 1.0 - (dist / maxLen);
    let part = s1.length < s2.length ? partialRatio(s1, s2) : partialRatio(s2, s1);

    let score = 0.4 * full + 0.6 * part;

    let lenDiff = Math.abs(s1.length - s2.length);
    if (lenDiff >= 10) {
        score -= 0.02 * lenDiff / maxLen;
    }

    let commonPrefixLen = 0;
    let minLen = min2(s1.length, s2.length);
    for (let i = 0; i < minLen; i++) {
        if (s1[i] === s2[i]) {
            commonPrefixLen++;
        } else {
            break;
        }
    }
    score += 0.01 * commonPrefixLen;

    if (s1.includes(s2) || s2.includes(s1)) {
        score += 0.2;
    }

    return Math.max(0.0, Math.min(1.0, score));
}
`;
const baseline = load(baselineSource);

// Frozen trim/banded-DP implementation immediately before the bit-vector path.
// Keep both baselines to distinguish this improvement from earlier changes.
const dpSource = baselineSource + String.raw`
function levenshteinDistance(s1, s2, rows) {
    return windowDistance(s1, s2, 0, s2.length, rows);
}

// Compare a window directly, without allocating a substring. Equal ends can
// be removed because they do not change Levenshtein distance (UTF-16 units).
function windowDistance(s1, s2, start2, len2, rows, limit) {
    let len1 = s1.length;
    let start1 = 0;
    while (len1 > 0 && len2 > 0 && s1[start1] === s2[start2]) {
        start1++;
        start2++;
        len1--;
        len2--;
    }
    while (len1 > 0 && len2 > 0 && s1[start1 + len1 - 1] === s2[start2 + len2 - 1]) {
        len1--;
        len2--;
    }

    if (len1 === 0) return len2;
    if (len2 === 0) return len1;

    if (len2 > len1) {
        [s1, s2] = [s2, s1];
        [len1, len2] = [len2, len1];
        [start1, start2] = [start2, start1];
    }

    let prev = rows ? rows[0] : new Array(len2 + 1);
    let curr = rows ? rows[1] : new Array(len2 + 1);

    const bound = limit === undefined ? len1 : limit;
    if (len1 - len2 > bound) return bound + 1;
    for (let j = 0; j <= len2; j++) {
        prev[j] = j;
    }

    // Wide bands save few cells, so use the simpler exact kernel instead.
    if (bound * 2 >= len2) {
        for (let i = 1; i <= len1; i++) {
            curr[0] = i;
            const char1 = s1[start1 + i - 1];
            for (let j = 1; j <= len2; j++) {
                let cost = char1 === s2[start2 + j - 1] ? 0 : 1;
                curr[j] = min3(prev[j] + 1, curr[j - 1] + 1, prev[j - 1] + cost);
            }
            const tmp = prev;
            prev = curr;
            curr = tmp;
        }
        return prev[len2];
    }

    for (let i = 1; i <= len1; i++) {
        curr[0] = i;
        // A path costing at most bound never leaves this diagonal band.
        const first = Math.max(1, i - bound);
        const last = Math.min(len2, i + bound);
        if (first > 1) curr[first - 1] = bound + 1;
        if (last < len2) curr[last + 1] = bound + 1;
        let rowMin = curr[0];
        const char1 = s1[start1 + i - 1];
        for (let j = first; j <= last; j++) {
            let cost = char1 === s2[start2 + j - 1] ? 0 : 1;
            curr[j] = min3(prev[j] + 1, curr[j - 1] + 1, prev[j - 1] + cost);
            if (curr[j] < rowMin) rowMin = curr[j];
        }
        // partialRatio needs only distances that improve its current best.
        if (rowMin > bound) return bound + 1;
        const tmp = prev;
        prev = curr;
        curr = tmp;
    }

    return prev[len2];
}

function partialRatio(shortS, longS) {
    let lenS = shortS.length;
    let lenL = longS.length;
    let bestDistance = lenS;

    if (lenS === 0) return 1.0;
    if (longS.includes(shortS)) return 1.0;
    if (lenS > lenL) return 0.0;

    // Every sliding window has the same size; reuse its distance workspace.
    const rows = [new Array(lenS + 1), new Array(lenS + 1)];

    for (let i = 0; i <= lenL - lenS; i++) {
        let dist = windowDistance(shortS, longS, i, lenS, rows, bestDistance - 1);
        if (dist < bestDistance) bestDistance = dist;
        // includes() already ruled out distance zero for every window.
        if (bestDistance === 1) break;
    }

    return 1.0 - (bestDistance / lenS);
}

`;
const dpBaseline = load(dpSource);

// Independent full-matrix oracle verifies the rolling-row baseline as well.
function matrixDistance(a, b) {
    const matrix = Array.from({ length: a.length + 1 }, (_, i) => [i]);
    for (let j = 0; j <= b.length; j++) matrix[0][j] = j;
    for (let i = 1; i <= a.length; i++) {
        for (let j = 1; j <= b.length; j++) {
            matrix[i][j] = Math.min(matrix[i - 1][j] + 1,
                matrix[i][j - 1] + 1, matrix[i - 1][j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
        }
    }
    return matrix[a.length][b.length];
}

function matrixPartialRatio(a, b) {
    if (a.length === 0) return 1;
    let best = 0;
    for (let start = 0; start <= b.length - a.length; start++) {
        best = Math.max(best, 1 - matrixDistance(a, b.slice(start, start + a.length)) / a.length);
    }
    return best;
}

let comparisons = 0;
const reusableRows = [[], []];
function check(a, b) {
    const expected = matrixDistance(a, b);
    const description = JSON.stringify([a, b]);
    assert.equal(baseline.levenshteinDistance(a, b), expected, "baseline distance " + description);
    assert.equal(actual.levenshteinDistance(a, b), expected, "distance " + description);
    // Alternate arbitrarily poisoned workspace between small and large calls.
    reusableRows[0].fill(NaN);
    reusableRows[1].fill(-9999);
    assert.equal(actual.levenshteinDistance(a, b, reusableRows), expected, "reused rows " + description);
    assert.equal(actual.partialRatio(a, b), matrixPartialRatio(a, b), "oracle partial " + description);
    for (const name of ["partialRatio", "computeScore", "computeTextMatchScore"]) {
        assert.equal(actual[name](a, b), baseline[name](a, b), name + " " + description);
        assert.equal(actual[name](a, b), dpBaseline[name](a, b), "DP " + name + " " + description);
    }
    comparisons++;
}

const exhaustive = [""];
let level = [""];
for (let length = 1; length <= 5; length++) {
    const next = [];
    for (const s of level) next.push(s + "a", s + "b");
    level = next;
    exhaustive.push(...level);
}
for (const a of exhaustive) for (const b of exhaustive) check(a, b);

const adversarial = ["", "a", "aa", "ab", "ba", "x".repeat(40), "a".repeat(40),
    "a".repeat(39) + "b", "b" + "a".repeat(39), "ababababab", "bababababa",
    "prefix-typo-suffix", "prefix-type-suffix", "😀", "😃", "😀x😀", "\ud83d", "\ude00",
    "a\u0000b", "e\u0301", "é", "ABC", "abc", "a".repeat(8) + "b" + "a".repeat(8)];
adversarial.push("conflguratlon", "prefix conflguratlon text prefix conflguratlon text",
    "configuration configuration", "aaaaabbbbbccccc", "aaaaacbbbbcccca");
for (const a of adversarial) for (const b of adversarial) check(a, b);

let state = 0x1a2b3c4d;
function random(max) {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    return (state >>> 0) % max;
}
const alphabet = ["a", "b", "c", " ", "\u0000", "\ud83d", "\ude00", "é", "\u0301"];
function randomString(length) {
    let result = "";
    for (let i = 0; i < length; i++) result += alphabet[random(alphabet.length)];
    return result;
}
for (let i = 0; i < 2500; i++) {
    const a = randomString(random(28));
    const b = i % 3 === 0 ? a.slice(0, random(a.length + 1)) + randomString(random(15)) : randomString(random(45));
    check(a, b);
    check(b, a);
}
// Test the sign bit, dispatch boundary and displaced windows, including
// surrogate units, NUL and combining marks. Each call constructs fresh masks.
let bitWindows = 0;
for (const pattern of exhaustive) {
    if (!pattern.length) continue;
    const masks = actual.shortPatternMasks(pattern);
    const before = JSON.stringify(masks);
    for (const text of exhaustive) {
        if (text.length !== pattern.length) continue;
        for (const offset of [0, 1]) {
            assert.equal(actual.bitWindowDistance(pattern.length, "z".repeat(offset) + text, offset, masks),
                matrixDistance(pattern, text), "exhaustive bit window");
            bitWindows++;
        }
    }
    assert.equal(JSON.stringify(masks), before, "masks are read-only across windows");
}
for (const length of [1, 2, 31, 32]) {
    for (let sample = 0; sample < 150; sample++) {
        const pattern = randomString(length);
        const text = randomString(length * 3 + 5);
        const masks = actual.shortPatternMasks(pattern);
        for (const offset of [0, 1, length, text.length - length]) {
            assert.equal(actual.bitWindowDistance(length, text, offset, masks),
                matrixDistance(pattern, text.slice(offset, offset + length)), "bit window at " + offset);
            bitWindows++;
        }
        check(pattern, text);
    }
}
for (let sample = 0; sample < 150; sample++) {
    const pattern = randomString(33);
    check(pattern, randomString(100));
}
for (const length of [31, 32, 33]) {
    const patterns = ["a".repeat(length), "\u0000".repeat(length),
        "a".repeat(length - 1) + "\ud83d", "\ude00" + "a".repeat(length - 1),
        ("😀e\u0301").repeat(9).slice(0, length)];
    for (const pattern of patterns) {
        for (const replacement of ["a", "b", "\u0000", "\ud83d", "\u0301"]) {
            const near = pattern.slice(0, -1) + replacement;
            check(pattern, "z".repeat(length) + near + "y".repeat(length));
            check(pattern, replacement.repeat(length * 3));
        }
    }
}
// Repeated unrelated patterns catch accidentally shared or uncleared masks.
for (let repeat = 0; repeat < 5; repeat++) {
    for (const pattern of ["a".repeat(32), "b".repeat(32), "\u0000".repeat(32), "\ud83d".repeat(32)]) {
        check(pattern, "z".repeat(96));
    }
}
console.log(`PASS: ${bitWindows} exact bit windows; masks stay unchanged across offsets.`);
console.log(`PASS: ${comparisons} pairs; distance, reusable rows, partial ratio and both scoring functions match exactly.`);

if (process.argv.includes("--benchmark")) {
    // Count evaluated DP cells independently of timings; avoids flaky timing
    // assertions while retaining a deterministic measure of algorithmic work.
    const marker = "curr[j] = min3(";
    function counted(code) {
        assert.ok(code.includes(marker));
        code = code.split(marker).join("cells++; " + marker);
        code = code.replace("let equal = masks[text.charCodeAt(i)]", "bitUnits++; let equal = masks[text.charCodeAt(i)]");
        code = code.replace("const code = pattern.charCodeAt(i)", "maskUnits++; const code = pattern.charCodeAt(i)");
        return load("var cells = 0, bitUnits = 0, maskUnits = 0;\n" + code);
    }
    const cases = [
        ["shared ends", "computeScore", [["org.example.document.viewer", "org.example.document.editor"]], 12000],
        ["launcher typos", "computeScore", [
            ["firefox", "firefix"], ["terminal", "gnome-terminal"], ["calender", "calendar"],
            ["chromium", "chromium-browser"], ["system settings", "system-settings"]], 2000],
        ["clipboard near match", "computeTextMatchScore", [["configuration", ("prefix configuratlonx extra text ").repeat(12)]], 20],
        ["clipboard two edits", "computeTextMatchScore", [["configuration", ("prefix conflguratlonx extra text ").repeat(12)]], 20],
        ["clipboard unrelated", "computeTextMatchScore", [["configuration", ("unrelated clipboard entry with many characters ").repeat(8)]], 20],
        ["partial near match", "partialRatio", [["configuration", ("prefix configuratlonx extra text ").repeat(12)]], 40],
        ["partial two edits", "partialRatio", [["configuration", ("prefix conflguratlonx extra text ").repeat(12)]], 40],
        ["partial unrelated", "partialRatio", [["configuration", ("unrelated clipboard entry with many characters ").repeat(8)]], 40],
        ["short unrelated", "computeScore", [["abc", "xyz"], ["firefox", "terminal"], ["settings", "chromium"]], 6000],
    ];
    function execute(api, name, pairs, iterations) {
        let checksum = 0;
        for (let i = 0; i < iterations; i++) {
            for (const pair of pairs) checksum += api[name](...pair);
        }
        return checksum;
    }
    function medianTime(api, name, pairs, iterations) {
        execute(api, name, pairs, Math.min(iterations, 1000));
        const times = [];
        for (let sample = 0; sample < 5; sample++) {
            const start = performance.now();
            execute(api, name, pairs, iterations);
            times.push(performance.now() - start);
        }
        return times.sort((a, b) => a - b)[2];
    }
    const beforeCells = counted(baselineSource);
    const dpCells = counted(dpSource);
    const afterCells = counted(source);
    for (const [label, name, pairs, iterations] of cases) {
        beforeCells.cells = 0;
        dpCells.cells = 0;
        afterCells.cells = 0;
        afterCells.bitUnits = 0;
        afterCells.maskUnits = 0;
        assert.equal(execute(beforeCells, name, pairs, 1), execute(afterCells, name, pairs, 1));
        const before = medianTime(baseline, name, pairs, iterations);
        const dp = medianTime(dpBaseline, name, pairs, iterations);
        const after = medianTime(actual, name, pairs, iterations);
        execute(dpCells, name, pairs, 1);
        console.log(`${label}: ${iterations * pairs.length} scores; ${before.toFixed(1)} -> ${dp.toFixed(1)} -> ${after.toFixed(1)} ms (HEAD/after ${(before / after).toFixed(2)}x; DP/after ${(dp / after).toFixed(2)}x); DP cells ${beforeCells.cells} -> ${dpCells.cells} -> ${afterCells.cells}; bit units ${afterCells.bitUnits}, mask units ${afterCells.maskUnits}`);
    }
}
