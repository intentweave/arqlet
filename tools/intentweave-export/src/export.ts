import { DatabaseSync } from 'node:sqlite';
import { posix } from 'node:path';

const claimTables = [
  'claim_identities', 'claim_versions', 'claim_assessments',
  'claim_assessment_references', 'claim_assessment_dependencies',
  'subject_identities', 'subject_aliases', 'claim_subjects',
  'parameter_identities', 'parameter_evidence_bindings',
  'evidence_identities', 'evidence_versions', 'evidence_subjects',
  'rule_result_identities', 'rule_result_versions', 'rule_result_evidence',
  'review_decisions', 'review_decision_reopens', 'subject_observations',
  'observed_identities', 'observed_versions',
];
const stable = (rows) => rows.sort((a,b) => {
  const x=JSON.stringify(a), y=JSON.stringify(b);
  return x < y ? -1 : x > y ? 1 : 0;
});
const key = (type, value) => `${type}:${value}`;

/** Read one consistent SQLite snapshot. Does not migrate or modify the source. */
export function exportIndex(dbPath, {includeClaims=false} = {}) {
  const db = new DatabaseSync(dbPath, {readOnly:true});
  try {
    db.exec('BEGIN');
    const tables = new Set(db.prepare("SELECT name FROM sqlite_master WHERE type='table'").all().map(r=>r.name));
    const required = {files:['path'], symbols:['id','name','file_path'], imports:['id','source_file','target_file'], symbol_calls:['id','caller_file','caller_name','callee_id']};
    for (const [table, columns] of Object.entries(required)) {
      if (!tables.has(table)) throw new Error(`Missing required table: ${table}`);
      const actual = new Set(db.prepare(`PRAGMA table_info("${table}")`).all().map(r=>r.name));
      for(const column of columns) if(!actual.has(column)) throw new Error(`Missing required column: ${table}.${column}`);
    }
    const read = table => stable(db.prepare(`SELECT * FROM "${table}"`).all().map(r=>({...r})));
    const metadata = tables.has('_meta') ? Object.fromEntries(read('_meta').map(r=>[r.key,r.value])) : {};
    const diagnostics = [];
    if(metadata.schema_version !== '23') diagnostics.push({kind:'unverified_schema',version:metadata.schema_version ?? null});
    const files=read('files'), symbols=read('symbols'), imports=read('imports'), calls=read('symbol_calls');
    const nodes = new Map();
    const add = node => { if(nodes.has(node.id)) throw new Error(`Duplicate node ID: ${node.id}`); nodes.set(node.id,node); };
    add({id:'directory:.',kind:'directory',label:'.',parentId:null,derived:true});
    const directory = path => {
      if(path==='.' || path==='') return 'directory:.';
      const id=key('directory',path);
      if(!nodes.has(id)) add({id,kind:'directory',label:posix.basename(path),parentId:directory(posix.dirname(path)),derived:true});
      return id;
    };
    const file = (path, source=null) => {
      const id=key('file',path);
      if(!nodes.has(id)) add({id,kind:'file',label:posix.basename(path),parentId:directory(posix.dirname(path)),sourceId:path,source,derived:source===null});
      return id;
    };
    for(const row of files) file(row.path,row);
    const byName=new Map();
    for(const row of symbols) {
      add({id:key('symbol',row.id),kind:'symbol',label:row.name,parentId:file(row.file_path),sourceId:row.id,source:row});
      const k=JSON.stringify([row.file_path,row.name]);
      byName.set(k,[...(byName.get(k)??[]),row]);
    }
    // Container names are hints, not stable IDs. Only adopt unambiguous acyclic parents.
    for(const row of symbols) if(row.container) {
      const matches=byName.get(JSON.stringify([row.file_path,row.container]))??[];
      if(matches.length===1 && matches[0].id!==row.id) {
        const child=nodes.get(key('symbol',row.id)), parent=key('symbol',matches[0].id);
        let cursor=parent;
        while(cursor && cursor!==child.id) cursor=nodes.get(cursor)?.parentId;
        if(cursor!==child.id) {child.parentId=parent; child.parentBasis='unique_container_name';}
        else diagnostics.push({kind:'cyclic_symbol_container',id:child.id});
      } else diagnostics.push({kind:'unresolved_symbol_container',id:key('symbol',row.id),container:row.container});
    }
    const edges=[];
    for(const row of imports) {
      const target=row.target_file ? key('file',row.target_file) : null;
      edges.push({id:key('import',row.id),kind:'imports',sourceId:row.id,source:file(row.source_file),target:nodes.has(target)?target:null,sourceRecord:row});
    }
    for(const row of calls) {
      const matches=byName.get(JSON.stringify([row.caller_file,row.caller_name]))??[];
      const source=matches.length===1 ? key('symbol',matches[0].id) : file(row.caller_file);
      const target=row.callee_id ? key('symbol',row.callee_id) : null;
      edges.push({id:key('call',row.id),kind:'calls',sourceId:row.id,source,target:nodes.has(target)?target:null,sourceResolution:matches.length===1?'unique_file_and_name':'file_fallback',sourceRecord:row});
    }
    for(const edge of edges) if(edge.target===null) diagnostics.push({kind:'unresolved_target',edgeId:edge.id});
    const graph={schemaVersion:'1',nodes:stable([...nodes.values()]),edges:stable(edges),diagnostics:stable(diagnostics)};
    validateGraph(graph);
    let claims=null;
    if(includeClaims) {
      const available=claimTables.filter(t=>tables.has(t));
      claims={schemaVersion:'1',representation:'intentweave-source-tables',tables:Object.fromEntries(available.map(t=>[t,read(t)])),missingTables:claimTables.filter(t=>!tables.has(t)),mappingStatus:'raw_subject_bindings_only'};
    }
    const manifest={schemaVersion:'1',exporter:{name:'intentweave-graph-export',version:'0.1.0'},source:{metadata,repositoryRevision:null,revisionStatus:'not_inferred_from_current_checkout'},scope:'complete_core_tables',counts:{nodes:graph.nodes.length,edges:edges.length,files:files.length,symbols:symbols.length,imports:imports.length,calls:calls.length,unresolvedTargets:edges.filter(e=>e.target===null).length},claims:includeClaims?{included:true,tableCounts:Object.fromEntries(Object.entries(claims.tables).map(([t,r])=>[t,r.length]))}:{included:false},limitations:['Source IDs may change on reindex; no cross-revision identity guarantee.','Directory containers are derived from source paths.','Claim-to-scene resolution is not implemented; raw bindings are preserved.','No source-index freshness guarantee.']};
    db.exec('COMMIT');
    return {manifest,graph,claims};
  } finally {db.close();}
}

export function validateGraph(graph) {
  const nodes=new Map();
  for(const n of graph.nodes) {if(nodes.has(n.id)) throw new Error(`Duplicate node ID: ${n.id}`); nodes.set(n.id,n);}
  for(const n of nodes.values()) {
    const seen=new Set([n.id]); let p=n.parentId;
    while(p!==null && p!==undefined) {
      if(!nodes.has(p)) throw new Error(`Missing parent: ${p}`);
      if(seen.has(p)) throw new Error(`Containment cycle: ${p}`);
      seen.add(p); p=nodes.get(p).parentId;
    }
  }
  const ids=new Set();
  for(const e of graph.edges) {
    if(ids.has(e.id)) throw new Error(`Duplicate edge ID: ${e.id}`); ids.add(e.id);
    if(!nodes.has(e.source)) throw new Error(`Missing source: ${e.source}`);
    if(e.target!==null && !nodes.has(e.target)) throw new Error(`Missing target: ${e.target}`);
  }
}
