#!/usr/bin/env node
// EXPECT: pass
// ISSUE: copy/v86#881
// DESC: file contents of an unlinked, closed file are released from the 9p filesystem

import assert from "node:assert/strict";
import { FS } from "../../../lib/filesystem.js";
import { MemoryFileStorage } from "../../../src/browser/filestorage.js";

process.on("unhandledRejection", exn => { throw exn; });

const storage = new MemoryFileStorage();
const fs = new FS(storage);

const data = new Uint8Array(1 << 20).fill(0x42);
await fs.CreateBinaryFile("big", 0, data);
const id = fs.SearchPath("/big").id;
assert.notEqual(id, -1);
assert.ok(fs.inodes[id].size === data.length);

assert.equal(fs.Unlink(0, "big"), 0);
fs.CloseInode(id);                              // last reference goes away

assert.equal(fs.SearchPath("/big").id, -1, "the name is gone");
const retained = (fs.inodedata[id] ? fs.inodedata[id].length : 0) + storage.filedata.size;
assert.equal(retained, 0, "no file data may stay in memory after unlink + close");
