#!/usr/bin/env node
// Exercise the actual QML command and collector without a desktop session.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const {execFileSync} = require("node:child_process");

const qml = fs.readFileSync(path.resolve(__dirname, "../services/ResourceUsage.qml"), "utf8");
const start = qml.indexOf("        id: findCpuMaxFreqProc\n");
assert(start >= 0, "Missing CPU frequency process");
const end = qml.indexOf("\n    Process {", start);
assert(end > start, "Missing next process");
const processSource = qml.slice(start, end);
const commandMatch = processSource.match(/command: (\[[^\n]+\])/);
assert(commandMatch, "Missing frequency command");
const command = JSON.parse(commandMatch[1]);
assert.deepEqual(command, ["/usr/bin/lscpu"]);
assert.match(processSource, /LANG: "C"/);
assert.match(processSource, /LC_ALL: "C"/);
assert.match(processSource, /running: false/);

const bodyStart = processSource.indexOf("onStreamFinished: {");
assert(bodyStart >= 0, "Missing frequency collector");
const bodyEnd = processSource.indexOf("\n            }", bodyStart);
assert(bodyEnd > bodyStart, "Missing collector end");
const state = {root: {maxAvailableCpuString: "--"}, outputCollector: {text: ""}};
const collect = vm.runInNewContext("(function () {" +
    processSource.slice(bodyStart + "onStreamFinished: {".length, bodyEnd) + "})", state);
function parse(text) {
    state.root.maxAvailableCpuString = "previous value";
    state.outputCollector.text = text;
    collect();
    return state.root.maxAvailableCpuString;
}

const cases = [
    ["CPU max MHz: 4800.0000\n", "5 GHz"],
    ["  CPU max MHz:\t3499.999\r\n", "3 GHz"],
    ["CPU max MHz: 3500\n", "4 GHz"],
    ["CPU max MHz: 9.99e2\n", "1 GHz"],
    ["CPU max MHz: .5\n", "0 GHz"],
    ["CPU max MHz: +2400\n", "2 GHz"],
    ["CPU max MHz: 0\n", "--"],
    ["CPU max MHz: -2400\n", "--"],
    ["CPU max MHz: unavailable\n", "--"],
    ["CPU max MHz: \nCPU MHz: 4000\n", "--"],
    ["CPU max MHz:\t\r\nCPU min MHz: 800\n", "--"],
    ["CPU max MHz: 2400\nCPU max MHz: 4800\n", "2 GHz"],
    ["CPU max MHz: invalid\nCPU max MHz: 4800\n", "--"],
    ["CPU max MHz: \nCPU max MHz: 4800\n", "--"],
    ["CPU MHz: 4800\nCPU min MHz: 800\nCPU(s) scaling MHz: 99%\nL3 cache: 32000\n", "--"],
    ["Other CPU max MHz: 4800\n", "--"],
    ["", "--"]
];
for (const [input, expected] of cases) assert.equal(parse(input), expected, JSON.stringify(input));
const positiveFrequencies = [Number.MIN_VALUE, 0.1, 999.999, 1499.999, 1500,
    2499.999, 2500, 9999.999, 1.23456789e6, Number.MAX_VALUE];
for (const mhz of positiveFrequencies) {
    assert.equal(parse("CPU max MHz:\t" + mhz + "\n"), (mhz / 1000).toFixed(0) + " GHz",
        "Retain rounding for finite positive frequency " + mhz);
}

const environment = Object.assign({}, process.env, {LANG: "C", LC_ALL: "C"});
const baselineCommand = ["/usr/bin/bash", "-c",
    "/usr/bin/lscpu | /usr/bin/grep 'CPU max MHz' | /usr/bin/awk '{print $4}'"];
function run(args) {
    return execFileSync(args[0], args.slice(1), {encoding: "utf8", env: environment});
}
function baselineParse(output) {
    const mhz = parseFloat(output);
    return isNaN(mhz) || mhz <= 0 ? "--" : (mhz / 1000).toFixed(0) + " GHz";
}
const actual = parse(run(command));
assert.equal(actual, baselineParse(run(baselineCommand)), "Real lscpu must match the previous pipeline");
console.log("CPU frequency: " + cases.length + " collector fixtures and " + positiveFrequencies.length +
    " finite positive rounding checks passed; real lscpu result " + actual);
console.log("Command structure: 4 executables (bash, lscpu, grep, awk) -> 1 (lscpu); process count inferred, not traced.");

if (process.argv.includes("--benchmark")) {
    const baselineTimes = [];
    const directTimes = [];
    function measure(args, parser, samples) {
        const before = process.hrtime.bigint();
        parser(run(args));
        samples.push(Number(process.hrtime.bigint() - before) / 1e6);
    }
    for (let i = 0; i < 9; i++) {
        if (i % 2 === 0) {
            measure(baselineCommand, baselineParse, baselineTimes);
            measure(command, parse, directTimes);
        } else {
            measure(command, parse, directTimes);
            measure(baselineCommand, baselineParse, baselineTimes);
        }
    }
    const median = samples => samples.slice().sort((a, b) => a - b)[4].toFixed(3);
    console.log("9 interleaved runs median: pipeline " + median(baselineTimes) +
        " ms; direct lscpu + production collector " + median(directTimes) + " ms (no timing threshold).");
}
