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
// Memberships across the hierarchy (Arqlet ADR 0022): documents, symbol kinds and visibility as dimensions.
function membershipFixture(fn) {
 const dir=mkdtempSync(join(tmpdir(),'iw-members-')),path=join(dir,'index.db');
 const db=new DatabaseSync(path);
 db.exec(`CREATE TABLE files(path TEXT PRIMARY KEY); INSERT INTO files VALUES('src/a.ts'),('src/b.ts');
 CREATE TABLE symbols(id TEXT PRIMARY KEY,name TEXT,kind TEXT,file_path TEXT,container TEXT,export TEXT);
 INSERT INTO symbols VALUES('a','A','function','src/a.ts',NULL,'exported'),('b','B','interface','src/b.ts',NULL,'exported'),
  ('c','C','function','src/b.ts',NULL,'internal');
 CREATE TABLE imports(id TEXT,source_file TEXT,target_file TEXT);
 CREATE TABLE symbol_calls(id TEXT,caller_file TEXT,caller_name TEXT,callee_id TEXT);
 INSERT INTO symbol_calls VALUES('1','src/a.ts','A','b'),('2','src/b.ts','C','a');
 CREATE TABLE _meta(key TEXT,value TEXT); INSERT INTO _meta VALUES('schema_version','23');
 CREATE TABLE annotations(id INTEGER PRIMARY KEY,doc_path TEXT,line INTEGER,text TEXT,symbol_id TEXT,confidence REAL,source TEXT);
 INSERT INTO annotations(doc_path,line,text,symbol_id,confidence,source) VALUES
  ('docs/design.md',1,'A',  'a',1.0,'identifier'),
  ('docs/design.md',2,'B',  'b',0.9,'identifier'),
  ('docs/design.md',3,'A',  'a',1.0,'identifier'),
  ('docs/usage.md', 1,'A',  'a',0.8,'identifier'),
  ('docs/usage.md', 2,'C',  'c',0.5,'heading'),
  ('docs/usage.md', 3,'gone','zz',1.0,'identifier'),
  ('docs/usage.md', 4,'none',NULL,1.0,'heading');`);
 db.close(); try{fn(path,dir);}finally{rmSync(dir,{recursive:true,force:true});}
}
test('memberships: documents across files, kinds and visibility, deterministic and opt-in',()=>membershipFixture(path=>{
 assert.equal(exportIndex(path).graph.memberships,undefined,'without the option the graph is as before');
 const {graph,manifest}=exportIndex(path,{memberships:true});
 const dim=id=>graph.memberships.dimensions.find(d=>d.id===id);
 assert.deepEqual(graph.memberships.dimensions.map(d=>d.id),['doc','kind','visibility']);
 // A symbol mentioned by two documents is in both; a document's symbols lie in two files; repeats count once.
 assert.deepEqual(dim('doc').groups,[
  {id:'docs/design.md',label:'docs/design.md',members:['symbol:a','symbol:b']},
  {id:'docs/usage.md',label:'docs/usage.md',members:['symbol:a']},
 ],'C is below the confidence, zz is no symbol, a row without a symbol is skipped');
 assert.deepEqual(dim('kind').groups.map(g=>[g.id,g.members]),[['function',['symbol:a','symbol:c']],['interface',['symbol:b']]]);
 assert.deepEqual(dim('visibility').groups.map(g=>[g.id,g.members]),[['exported',['symbol:a','symbol:b']],['internal',['symbol:c']]]);
 assert.deepEqual(manifest.memberships,{included:true,docConfidence:0.8,missing:[],dimensions:{doc:{groups:2,memberships:3},kind:{groups:2,memberships:3},visibility:{groups:2,memberships:3}}});
 // A lower threshold takes C in; the same snapshot gives the same output.
 const low=exportIndex(path,{memberships:true,docConfidence:0.5});
 assert.deepEqual(low.graph.memberships.dimensions[0].groups[1].members,['symbol:a','symbol:c']);
 assert.deepEqual(exportIndex(path,{memberships:true}),exportIndex(path,{memberships:true}));
 assert.throws(()=>exportIndex(path,{memberships:true,docConfidence:2}),/between 0 and 1/);
}));
test('memberships: an index without annotations still exports kinds, and names what it lacks',()=>fixture(path=>{
 const {graph,manifest}=exportIndex(path,{memberships:true});
 assert.deepEqual(manifest.memberships.missing,['annotations','symbols.kind','symbols.export']);
 assert.deepEqual(graph.memberships.dimensions,[]);
}));
test('validator rejects memberships Arqlet would refuse',()=>{
 const nodes=[{id:'a',parentId:null}];
 const with_=dimensions=>({nodes,edges:[],memberships:{dimensions}});
 assert.throws(()=>validateGraph(with_([{id:'parent',groups:[]}])),/parent/);
 assert.throws(()=>validateGraph(with_([{id:'d',groups:[]},{id:'d',groups:[]}])),/Duplicate membership dimension/);
 assert.throws(()=>validateGraph(with_([{id:'d',groups:[{id:'g',members:[]},{id:'g',members:[]}]}])),/Duplicate membership group/);
 assert.throws(()=>validateGraph(with_([{id:'d',groups:[{id:'g',members:['b']}]}])),/Missing member/);
});
test('CLI: --memberships writes them, --doc-confidence needs it',()=>membershipFixture((path,dir)=>{
 const run=(out,...args)=>spawnSync(process.execPath,['src/cli.ts','--db',path,'--out',join(dir,out),...args],{encoding:'utf8'});
 assert.equal(run('m','--memberships','--doc-confidence','0.5').status,0);
 const graph=JSON.parse(readFileSync(join(dir,'m','graph.json'),'utf8'));
 assert.equal(graph.memberships.dimensions[0].groups[1].members.length,2);
 const refused=run('n','--doc-confidence','0.5');
 assert.equal(refused.status,1); assert.match(refused.stderr,/needs --memberships/);
}));
test('fails clearly on incompatible schema',()=>{
 const dir=mkdtempSync(join(tmpdir(),'iw-empty-')),path=join(dir,'index.db');
 const db=new DatabaseSync(path); db.exec('CREATE TABLE other(id TEXT)');db.close();
 try{assert.throws(()=>exportIndex(path),/Missing required table/);}finally{rmSync(dir,{recursive:true,force:true});}
});
