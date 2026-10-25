#!/usr/bin/env node
// EXPECT: pass
// ISSUE: copy/v86#659
// DESC: reading an empty file from the 9p filesystem returns an empty array, not null/FileNotFound

import assert from "node:assert/strict";
import { FS } from "../../../lib/filesystem.js";
import { MemoryFileStorage } from "../../../src/browser/filestorage.js";

process.on("unhandledRejection", exn => { throw exn; });

const fs = new FS(new MemoryFileStorage());

await fs.CreateBinaryFile("empty", 0, new Uint8Array(0));
await fs.CreateBinaryFile("full", 0, new Uint8Array([1, 2, 3]));

const full = await fs.read_file("/full");
assert.deepEqual(Array.from(full), [1, 2, 3], "sanity: non-empty file reads back");

const empty = await fs.read_file("/empty");
assert.notEqual(empty, null, "empty file must not be reported as missing (V86.read_file would reject with FileNotFoundError)");
assert.equal(empty.length, 0);

assert.equal(await fs.read_file("/does-not-exist"), null, "missing files still return null");
