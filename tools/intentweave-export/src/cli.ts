import { parseArgs } from 'node:util';
import { mkdir, mkdtemp, writeFile, rename, rm, lstat } from 'node:fs/promises';
import { resolve, dirname, basename, join } from 'node:path';
import { createHash } from 'node:crypto';
import { exportIndex } from './export.ts';

try {
  const {values} = parseArgs({options:{db:{type:'string'},out:{type:'string'},claims:{type:'boolean',default:false},help:{type:'boolean'}}});
  if(values.help) console.log('Usage: npm run export -- --db /path/index.db --out /new/export-directory [--claims]');
  else {
    if(!values.db || !values.out) throw new Error('--db and --out are required');
    const out=resolve(values.out);
    try {await lstat(out); throw new Error(`Output already exists: ${out}`);} catch(e) {if(e.code!=='ENOENT') throw e;}
    const result=exportIndex(resolve(values.db),{includeClaims:values.claims});
    await mkdir(dirname(out),{recursive:true});
    const staging=await mkdtemp(join(dirname(out),`.${basename(out)}-`));
    try {
      const files={};
      for(const name of ['graph','claims']) if(result[name]!==null) {
        const text=JSON.stringify(result[name],null,2)+'\n';
        await writeFile(join(staging,`${name}.json`),text);
        files[`${name}.json`]={sha256:createHash('sha256').update(text).digest('hex'),bytes:Buffer.byteLength(text)};
      }
      await writeFile(join(staging,'manifest.json'),JSON.stringify({...result.manifest,files},null,2)+'\n');
      // Reserve destination exclusively, then move files into it. Never overwrite an existing export.
      await mkdir(out);
      for(const name of [...Object.keys(files),'manifest.json']) await rename(join(staging,name),join(out,name));
      console.log(JSON.stringify({output:out,...result.manifest.counts}));
    } finally {await rm(staging,{recursive:true,force:true});}
  }
} catch(e) {console.error(e.message); process.exitCode=1;}
