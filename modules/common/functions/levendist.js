// Original code from https://github.com/koeqaife/hyprland-material-you
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

// Short-pattern kernel adapted from fastest-levenshtein's myers_32:
// https://github.com/ka-weihe/fastest-levenshtein/blob/master/mod.ts
/*
MIT License

Copyright (c) 2020 Kasper Unn Weihe

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
*/
function shortPatternMasks(pattern) {
    const masks = Object.create(null);
    for (let i = 0; i < pattern.length; i++) {
        const code = pattern.charCodeAt(i);
        masks[code] = (masks[code] || 0) | (1 << i);
    }
    return masks;
}

// Each bit represents one pattern UTF-16 unit. A signed high bit at length
// 32 is intentional: all masks and shifts use JavaScript's 32-bit operations.
function bitWindowDistance(patternLength, text, start, masks) {
    let positive = -1;
    let negative = 0;
    let distance = patternLength;
    const last = 1 << (patternLength - 1);
    for (let i = start; i < start + patternLength; i++) {
        let equal = masks[text.charCodeAt(i)] || 0;
        const vertical = equal | negative;
        equal |= ((equal & positive) + positive) ^ positive;
        negative |= ~(equal | positive);
        positive &= equal;
        if (negative & last) distance++;
        if (positive & last) distance--;
        negative = (negative << 1) | 1;
        positive = (positive << 1) | ~(vertical | negative);
        negative &= vertical;
    }
    return distance;
}

function partialRatio(shortS, longS) {
    let lenS = shortS.length;
    let lenL = longS.length;
    let bestDistance = lenS;

    if (lenS === 0) return 1.0;
    if (longS.includes(shortS)) return 1.0;
    if (lenS > lenL) return 0.0;

    // Reuse one local mask table across many windows. For few windows, retain
    // the DP path so building masks cannot dominate short launcher matches.
    const masks = lenS <= 32 && lenL >= lenS * 2 ? shortPatternMasks(shortS) : null;
    const rows = masks ? null : [new Array(lenS + 1), new Array(lenS + 1)];

    for (let i = 0; i <= lenL - lenS; i++) {
        const dist = masks ? bitWindowDistance(lenS, longS, i, masks)
                           : windowDistance(shortS, longS, i, lenS, rows, bestDistance - 1);
        if (dist < bestDistance) bestDistance = dist;
        // includes() already ruled out distance zero for every window.
        if (bestDistance === 1) break;
    }

    return 1.0 - (bestDistance / lenS);
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
