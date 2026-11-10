import {test} from 'node:test';
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {mkdtempSync,rmSync,readFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {spawnSync} from 'node:child_process';
import {exportIndex,validateGraph} from '../src/export.ts';

function fixture(fn) {
 const dir=mkdtempSync(join(tmpdir(),'iw-export-')),path=join(dir,'index.db');
 const db=new DatabaseSync(path);
 db.exec(`CREATE TABLE files(path TEXT PRIMARY KEY); INSERT INTO files VALUES('src/a.ts');
 CREATE TABLE symbols(id TEXT PRIMARY KEY,name TEXT,file_path TEXT,container TEXT);
 INSERT INTO symbols VALUES('a','A','src/a.ts',NULL),('b','B','src/a.ts','A');
 CREATE TABLE imports(id TEXT,source_file TEXT,target_file TEXT); INSERT INTO imports VALUES('1','src/a.ts','node:fs');
 CREATE TABLE symbol_calls(id TEXT,caller_file TEXT,caller_name TEXT,callee_id TEXT);
 INSERT INTO symbol_calls VALUES('1','src/a.ts','A','a'),('2','src/a.ts','A','b'),('3','src/a.ts','A','b'),('4','src/a.ts','B','a');
 CREATE TABLE _meta(key TEXT,value TEXT); INSERT INTO _meta VALUES('schema_version','23');
 CREATE TABLE claim_identities(id TEXT); INSERT INTO claim_identities VALUES('claim-1');`);
 db.close(); try{fn(path,dir);}finally{rmSync(dir,{recursive:true,force:true});}
}
test('preserves loops, parallel edges, cycles, containment and unresolved targets',()=>fixture(path=>{
 const before=readFileSync(path); const a=exportIndex(path,{includeClaims:true});
 assert.equal(a.graph.edges.length,5);
 assert.equal(a.graph.nodes.find(n=>n.id==='symbol:b').parentId,'symbol:a');
 assert.equal(a.graph.edges.find(e=>e.id==='call:1').source,'symbol:a');
 assert.equal(a.graph.edges.find(e=>e.id==='call:1').target,'symbol:a');
 assert.equal(a.manifest.counts.unresolvedTargets,1);
 assert.equal(a.claims.tables.claim_identities.length,1);
 assert.ok(a.claims.missingTables.includes('claim_versions'));
 assert.deepEqual(a,exportIndex(path,{includeClaims:true}));
 assert.deepEqual(before,readFileSync(path));
}));
test('validator rejects duplicate IDs, missing endpoints and containment cycles',()=>{
 const node={id:'a',parentId:null};
 assert.throws(()=>validateGraph({nodes:[node,node],edges:[]}),/Duplicate/);
 assert.throws(()=>validateGraph({nodes:[node],edges:[{id:'e',source:'a',target:'b'}]}),/Missing target/);
 assert.throws(()=>validateGraph({nodes:[{id:'a',parentId:'b'},{id:'b',parentId:'a'}],edges:[]}),/cycle/);
});
test('CLI writes files and refuses overwrite',()=>fixture((path,dir)=>{
 const out=join(dir,'export');
 const run=()=>spawnSync(process.execPath,['src/cli.ts','--db',path,'--out',out],{encoding:'utf8'});
 assert.equal(run().status,0);
 const manifest=JSON.parse(readFileSync(join(out,'manifest.json'),'utf8'));
 assert.equal(manifest.claims.included,false); assert.ok(manifest.files['graph.json'].sha256);
 assert.equal(run().status,1);
}));
test('fails clearly on incompatible schema',()=>{
 const dir=mkdtempSync(join(tmpdir(),'iw-empty-')),path=join(dir,'index.db');
 const db=new DatabaseSync(path); db.exec('CREATE TABLE other(id TEXT)');db.close();
 try{assert.throws(()=>exportIndex(path),/Missing required table/);}finally{rmSync(dir,{recursive:true,force:true});}
});
