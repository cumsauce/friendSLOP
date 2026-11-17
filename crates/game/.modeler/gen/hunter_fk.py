import json, math, sys
R = json.load(open('' + __import__('os').path.join(__import__('os').path.dirname(__file__), 'hunter_rest.json') + ''))
for b in list(R):
    if b.endswith('_l'):
        h, t = R[b]; R[b[:-2]+'_r'] = [[-h[0],h[1],h[2]],[-t[0],t[1],t[2]]]
P = {"spine":"hip","chest":"spine","neck":"chest","head":"neck","jaw":"head"}
for s in "lr":
    P.update({f"shoulder_{s}":"chest",f"elbow_{s}":f"shoulder_{s}",f"wrist_{s}":f"elbow_{s}",f"hand_{s}":f"wrist_{s}",f"claw_{s}":f"hand_{s}",
              f"thigh_{s}":"hip",f"knee_{s}":f"thigh_{s}",f"ankle_{s}":f"knee_{s}",f"ball_{s}":f"ankle_{s}",f"toe_{s}":f"ball_{s}"})
ORDER = ["hip","spine","chest","neck","head","jaw"] + [f"{n}_{s}" for s in "lr" for n in ["shoulder","elbow","wrist","hand","claw","thigh","knee","ankle","ball","toe"]]
def qmul(a,b):
    ax,ay,az,aw=a; bx,by,bz,bw=b
    return (aw*bx+ax*bw+ay*bz-az*by, aw*by-ax*bz+ay*bw+az*bx, aw*bz+ax*by-ay*bx+az*bw, aw*bw-ax*bx-ay*by-az*bz)
def qaxis(ax,deg):
    h=math.radians(deg)/2; s=math.sin(h); return (ax[0]*s,ax[1]*s,ax[2]*s,math.cos(h))
def euler(e):
    q=qaxis((1,0,0),e.get("x",0)); q=qmul(qaxis((0,1,0),e.get("y",0)),q); return qmul(qaxis((0,0,1),e.get("z",0)),q)
def rot(q,v):
    x,y,z,w=q; p=qmul(qmul(q,(v[0],v[1],v[2],0)),(-x,-y,-z,w)); return p[:3]
def sub(a,b): return [a[i]-b[i] for i in range(3)]
def add(a,b): return [a[i]+b[i] for i in range(3)]
def pose(local, hip_t):
    """local: bone -> euler dict; returns bone -> (head, tail, worldq)"""
    out={}
    for b in ORDER:
        L=euler(local.get(b,{}))
        rh,rt=R[b]
        if b=="hip":
            W=L; h=add(rh,hip_t)
        else:
            p=P[b]; ph,_,pW=out[p]; W=qmul(pW,L); h=add(ph,rot(pW,sub(rh,R[p][0])))
        out[b]=(h,add(h,rot(W,sub(rt,rh))),W)
    return out
