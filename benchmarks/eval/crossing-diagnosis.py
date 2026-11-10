import json,os,collections,sys
D=os.environ['CLAUDE_JOB_DIR']+'/tmp/diag/'
EPS=1e-6
def flow_axis(graph, node, parents):
    p=parents.get(node)
    d=(graph.get('containerDirections') or {}).get(p, graph.get('defaultDirection')) if p else graph.get('defaultDirection')
    return {'lr':'h','rl':'h','tb':'v','bt':'v'}.get(d)
def segments(route):
    pts=route['points']; out=[]
    for i in range(len(pts)-1):
        a,b=pts[i],pts[i+1]
        if abs(a['y']-b['y'])<EPS and abs(a['x']-b['x'])>EPS: out.append(('h',a['y'],min(a['x'],b['x']),max(a['x'],b['x']),i,len(pts)-1))
        elif abs(a['x']-b['x'])<EPS and abs(a['y']-b['y'])>EPS: out.append(('v',a['x'],min(a['y'],b['y']),max(a['y'],b['y']),i,len(pts)-1))
    return out
def seg_class(seg, axis):
    kind,_,_,_,i,n=seg
    if i==0 or i==n-1: return 'port'
    if axis is None: return 'other'
    return 'along' if kind==axis else 'across'
def crossings(snapshot, graph):
    parents={n['id']:n.get('parentId') for n in snapshot['graph']['nodes']}
    edges={e['id']:e for e in snapshot['graph']['edges']}
    routes=[r for r in snapshot['routes'] if not r.get('bundle_of') and not r.get('bundleOf')]
    segs={r['id']:segments(r) for r in routes}
    axis={r['id']:flow_axis(graph, edges[r['id']]['source'], parents) if r['id'] in edges else None for r in routes}
    ids=list(segs); out=[]
    for i in range(len(ids)):
        for j in range(i+1,len(ids)):
            a,b=ids[i],ids[j]
            ea,eb=edges.get(a),edges.get(b)
            if ea and eb and len({ea['source'],ea.get('target')} & {eb['source'],eb.get('target')})>0: continue
            for s in segs[a]:
                for t in segs[b]:
                    if s[0]==t[0]: continue
                    h,v=(s,t) if s[0]=='h' else (t,s)
                    if v[1]>h[2]+EPS and v[1]<h[3]-EPS and h[1]>v[2]+EPS and h[1]<v[3]-EPS:
                        out.append((a,b,seg_class(s,axis[a]),seg_class(t,axis[b])))
    return out
total=collections.Counter()
for name in sys.argv[1:]:
    graph=json.load(open(f'benchmarks/datasets/{name}.json'))
    off=json.load(open(D+f'{name}-off.json')); on=json.load(open(D+f'{name}-on.json'))
    cls={e['id']:e['constructed'] for e in json.load(open(D+f'{name}-classes.json'))['edges']}
    c_off=crossings(off,graph); c_on=crossings(on,graph)
    pairs_off={tuple(sorted((a,b))) for a,b,_,_ in c_off}
    kinds=collections.Counter(); segk=collections.Counter(); new=0
    for a,b,sa,sb in c_on:
        ka='C' if cls.get(a) else 'G'; kb='C' if cls.get(b) else 'G'
        kinds[''.join(sorted(ka+kb))]+=1
        segk[' + '.join(sorted((sa,sb)))]+=1
        new+= tuple(sorted((a,b))) not in pairs_off
    print(f"== {name}: crossing points off {len(c_off)} on {len(c_on)}; on: new pairs' points {new}")
    print('   by route kinds (C constructed, G general):', dict(kinds))
    print('   by segment kinds:', dict(segk.most_common()))
    for k,v in kinds.items(): total['kind '+k]+=v
    for k,v in segk.items(): total['seg '+k]+=v
    total['off']+=len(c_off); total['on']+=len(c_on); total['new']+=new
print('TOTAL', dict(total))
