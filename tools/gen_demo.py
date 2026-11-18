"""Generates demo IFC4 models: a 3-storey building (architecture+structure) and MEP. Usage: gen_demo.py OUTDIR [BLOCKS]"""
import sys, os
out = sys.argv[1]
BLOCKS = int(sys.argv[2]) if len(sys.argv) > 2 else 1
OX = [0.0]
class W:
    def __init__(s): s.lines=[]; s.n=0
    def add(s, txt):
        s.n+=1; s.lines.append(f"#{s.n}={txt};"); return f"#{s.n}"
def header(name, view):
    return f"""ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('ViewDefinition [{view}]'),'2;1');
FILE_NAME('{name}','2026-11-14T10:00:00',('BimRust'),('BimRust'),'','BimRust demo','');
FILE_SCHEMA(('IFC4'));
ENDSEC;
DATA;
"""
def base(w, proj):
    c={}
    c['app']=w.add("IFCAPPLICATION($,'1.0','BimRust Demo Generator','BIMR')")
    u=w.add("IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.)")
    ua=w.add(f"IFCUNITASSIGNMENT(({u}))")
    c['o']=w.add("IFCCARTESIANPOINT((0.,0.,0.))")
    c['z']=w.add("IFCDIRECTION((0.,0.,1.))"); c['x']=w.add("IFCDIRECTION((1.,0.,0.))"); c['y']=w.add("IFCDIRECTION((0.,1.,0.))")
    c['ax']=w.add(f"IFCAXIS2PLACEMENT3D({c['o']},{c['z']},{c['x']})")
    c['ctx']=w.add(f"IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-05,{c['ax']},$)")
    c['body']=w.add(f"IFCGEOMETRICREPRESENTATIONSUBCONTEXT('Body','Model',*,*,*,*,{c['ctx']},$,.MODEL_VIEW.,$)")
    c['proj']=w.add(f"IFCPROJECT('{gid()}',$,'{proj}',$,$,$,$,({c['ctx']}),{ua})")
    c['root']=w.add(f"IFCLOCALPLACEMENT($,{c['ax']})")
    site=w.add(f"IFCSITE('{gid()}',$,'Site',$,$,{c['root']},$,$,.ELEMENT.,$,$,$,$,$)")
    bld=w.add(f"IFCBUILDING('{gid()}',$,'Block A',$,$,{c['root']},$,$,.ELEMENT.,$,$,$)")
    w.add(f"IFCRELAGGREGATES('{gid()}',$,$,$,{c['proj']},({site}))")
    w.add(f"IFCRELAGGREGATES('{gid()}',$,$,$,{site},({bld}))")
    c['bld']=bld
    return c
_g=[0]
def gid():
    _g[0]+=1
    import hashlib
    a="0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$"
    h=hashlib.sha1(f"bimrust-{_g[0]}".encode()).digest()
    return "".join(a[b%64] for b in h[:22]) + "x"*(22-len(h[:22]))
def pt(w,x,y,z): return w.add(f"IFCCARTESIANPOINT(({x:.1f},{y:.1f},{z:.1f}))")
def place(w,c,x,y,z,xdir=None,zdir=None):
    p=pt(w,x+OX[0],y,z)
    zd=zdir or c['z']; xd=xdir or c['x']
    a=w.add(f"IFCAXIS2PLACEMENT3D({p},{zd},{xd})")
    return w.add(f"IFCLOCALPLACEMENT({c['root']},{a})")
def box(w,c,dx,dy,dz,cx=None,cy=None):
    p2=w.add(f"IFCCARTESIANPOINT(({(cx if cx is not None else dx/2):.1f},{(cy if cy is not None else dy/2):.1f}))")
    a2=w.add(f"IFCAXIS2PLACEMENT2D({p2},$)")
    pr=w.add(f"IFCRECTANGLEPROFILEDEF(.AREA.,$,{a2},{dx:.1f},{dy:.1f})")
    ex=w.add(f"IFCEXTRUDEDAREASOLID({pr},{c['ax']},{c['z']},{dz:.1f})")
    return shape(w,c,ex)
def shape(w,c,item,kind='SweptSolid'):
    r=w.add(f"IFCSHAPEREPRESENTATION({c['body']},'Body','{kind}',({item}))")
    return w.add(f"IFCPRODUCTDEFINITIONSHAPE($,$,({r}))")
def style(w,item,r,g,b,t=0.0):
    col=w.add(f"IFCCOLOURRGB($,{r},{g},{b})")
    sr=w.add(f"IFCSURFACESTYLERENDERING({col},{t},$,$,$,$,$,$,.FLAT.)")
    ss=w.add(f"IFCSURFACESTYLE($,.BOTH.,({sr}))")
    w.add(f"IFCSTYLEDITEM({item},({ss}),$)")
def pset(w,objs,name,props):
    ps=[]
    for k,v in props.items():
        if isinstance(v,bool): val=f"IFCBOOLEAN(.{'T' if v else 'F'}.)"
        elif isinstance(v,(int,float)): val=f"IFCREAL({float(v)})"
        else: val=f"IFCLABEL('{v}')"
        ps.append(w.add(f"IFCPROPERTYSINGLEVALUE('{k}',$,{val},$)"))
    s=w.add(f"IFCPROPERTYSET('{gid()}',$,'{name}',$,({','.join(ps)}))")
    w.add(f"IFCRELDEFINESBYPROPERTIES('{gid()}',$,$,$,({','.join(objs)}),{s})")
def material(w,objs,name):
    m=w.add(f"IFCMATERIAL('{name}',$,$)")
    w.add(f"IFCRELASSOCIATESMATERIAL('{gid()}',$,$,$,({','.join(objs)}),{m})")

# ------------------------------------------------------------ architecture + structure
LX, LY, H = 24000.0, 15000.0, 3300.0
GX=[0,6000,12000,18000,24000]; GY=[0,7500,15000]
w=W(); c=base(w,'Block A - Architecture and Structure')
storeys=[]
for i,n in enumerate(['Ground Floor','Level 1','Level 2']):
    p=place(w,c,0,0,i*H)
    s=w.add(f"IFCBUILDINGSTOREY('{gid()}',$,'{n}',$,$,{p},$,$,.ELEMENT.,{i*H:.1f})")
    storeys.append((s,i*H,[]))
blocks_storeys=storeys
w.add(f"IFCRELAGGREGATES('{gid()}',$,$,$,{c['bld']},({','.join(s[0] for s in storeys)}))")
roofz=3*H
walls=[];slabs=[];cols=[];beams=[];wins=[];doors=[];stairs=[]
colshape=box(w,c,400,400,H-250,0,0)
for bk in range(BLOCKS):
    OX[0]=bk*30000.0
    for si,(st,z,elems) in enumerate(storeys):
        # placa
        sl=w.add(f"IFCSLAB('{gid()}',$,'Slab L{si} (concrete 250)',$,$,{place(w,c,0,0,z-250)},{box(w,c,LX,LY,250)},$,.FLOOR.)"); elems.append(sl); slabs.append(sl)
        for gx in GX:
            for gy in GY:
                co=w.add(f"IFCCOLUMN('{gid()}',$,'Column {chr(65+GY.index(gy))}{GX.index(gx)+1} L{si}',$,$,{place(w,c,gx,gy,z)},{colshape},$,.COLUMN.)"); elems.append(co); cols.append(co)
        # grinzi pe axele Y (lungime 6m)
        for gy in GY:
            for k in range(len(GX)-1):
                b=w.add(f"IFCBEAM('{gid()}',$,'Beam {chr(65+GY.index(gy))}{k+1}-{k+2} L{si}',$,$,{place(w,c,GX[k],gy,z+H-250-500)},{box(w,c,6000,300,500,3000,0)},$,.BEAM.)"); elems.append(b); beams.append(b)
        # exterior walls
        # south facade: vertical profile with window/door voids, extruded along -Y
        def poly2(pts):
            ps=[w.add(f"IFCCARTESIANPOINT(({x:.1f},{y:.1f}))") for x,y in pts]
            return w.add(f"IFCPOLYLINE(({','.join(ps+[ps[0]])}))")
        hh=H-250
        holes=[poly2([(1500+k*6000,900),(3300+k*6000,900),(3300+k*6000,2400),(1500+k*6000,2400)]) for k in range(4)]
        if si==0: holes.append(poly2([(11400,0.5),(12600,0.5),(12600,2200),(11400,2200)]))
        outer=poly2([(-300,0),(LX+300,0),(LX+300,hh),(-300,hh)])
        prof=w.add(f"IFCARBITRARYPROFILEDEFWITHVOIDS(.AREA.,$,{outer},({','.join(holes)}))")
        my=w.add("IFCDIRECTION((0.,-1.,0.))")
        pos=w.add(f"IFCAXIS2PLACEMENT3D({pt(w,0,-50,0)},{my},{c['x']})")
        ex=w.add(f"IFCEXTRUDEDAREASOLID({prof},{pos},{c['z']},250.)")
        wa=w.add(f"IFCWALL('{gid()}',$,'Wall South L{si}',$,$,{place(w,c,0,0,z)},{shape(w,c,ex)},$,.STANDARD.)"); elems.append(wa); walls.append(wa)
        for (x,y,dx,dy,nm) in [(0,LY+50,LX,250,'North'),(-300,0,250,LY,'West'),(LX+50,0,250,LY,'East')]:
            wa=w.add(f"IFCWALL('{gid()}',$,'Wall {nm} L{si}',$,$,{place(w,c,x,y,z)},{box(w,c,dx,dy,H-250)},$,.STANDARD.)"); elems.append(wa); walls.append(wa)
        # interior walls
        for (x,y,dx,dy) in [(200,7400,5600,120),(12200,7400,5600,120),(9000,200,120,7000)]:
            wa=w.add(f"IFCWALL('{gid()}',$,'Partition Wall L{si}',$,$,{place(w,c,x,y,z)},{box(w,c,dx,dy,H-250)},$,.PARTITIONING.)"); elems.append(wa); walls.append(wa)
        # windows (transparent glass) on the south facade
        for k in range(4):
            p2=w.add("IFCCARTESIANPOINT((900.,40.))"); a2=w.add(f"IFCAXIS2PLACEMENT2D({p2},$)")
            pr=w.add(f"IFCRECTANGLEPROFILEDEF(.AREA.,$,{a2},1800.,80.)")
            ex=w.add(f"IFCEXTRUDEDAREASOLID({pr},{c['ax']},{c['z']},1500.)")
            style(w,ex,0.55,0.75,0.95,0.55)
            wi=w.add(f"IFCWINDOW('{gid()}',$,'Window W{si}{k+1}',$,$,{place(w,c,1500+k*6000,-215,z+900)},{shape(w,c,ex)},$,1500.,1800.,.WINDOW.,$,$)"); elems.append(wi); wins.append(wi)
        if si==0:
            d=w.add(f"IFCDOOR('{gid()}',$,'Main Entrance Door',$,$,{place(w,c,11400,-200,0)},{box(w,c,1200,60,2200)},$,2200.,1200.,.DOOR.,$,$)"); elems.append(d); doors.append(d)
    # roof
    rf=w.add(f"IFCROOF('{gid()}',$,'Flat Roof',$,$,{place(w,c,-400,-400,roofz-250)},{box(w,c,LX+800,LY+800,300)},$,.FLAT_ROOF.)"); storeys[2][2].append(rf)
    # stair (steps)
    items=[]
    for k in range(18):
        p2=w.add(f"IFCCARTESIANPOINT((0.,0.))"); a2=w.add(f"IFCAXIS2PLACEMENT2D({p2},$)")
        pr=w.add(f"IFCRECTANGLEPROFILEDEF(.AREA.,$,{a2},1200.,280.)")
        pp=pt(w,0,k*280.0,k*183.0); ap=w.add(f"IFCAXIS2PLACEMENT3D({pp},$,$)")
        items.append(w.add(f"IFCEXTRUDEDAREASOLID({pr},{ap},{c['z']},183.)"))
    r=w.add(f"IFCSHAPEREPRESENTATION({c['body']},'Body','SweptSolid',({','.join(items)}))")
    ps=w.add(f"IFCPRODUCTDEFINITIONSHAPE($,$,({r}))")
    stair=w.add(f"IFCSTAIRFLIGHT('{gid()}',$,'Stair S1 Ground Floor',$,$,{place(w,c,20400,9000,0)},{ps},$,$,$,$,$,.STRAIGHT.)"); storeys[0][2].append(stair)
OX[0]=0.0
for st,z,elems in storeys:
    w.add(f"IFCRELCONTAINEDINSPATIALSTRUCTURE('{gid()}',$,$,$,({','.join(elems)}),{st})")
material(w,slabs+cols+beams+[rf],'Concrete C30/37'); material(w,walls,'AAC Masonry 250'); material(w,wins,'Insulated Glazing')
pset(w,walls,'Pset_WallCommon',{'IsExternal':True,'FireRating':'REI 120','LoadBearing':False})
pset(w,cols+beams,'Pset_Structural',{'ConcreteClass':'C30/37','Phase':'Structure'})
pset(w,wins,'Pset_WindowCommon',{'ThermalTransmittance':1.1,'IsExternal':True})
open(os.path.join(out,'Block_A_Architecture_Structure.ifc'),'w').write(header('Block_A_Architecture_Structure.ifc','CoordinationView')+"\n".join(w.lines)+"\nENDSEC;\nEND-ISO-10303-21;\n")

# ------------------------------------------------------------ MEP
w=W(); c=base(w,'Block A - MEP')
storeys=[]
for i,n in enumerate(['Ground Floor','Level 1','Level 2']):
    p=place(w,c,0,0,i*H)
    s=w.add(f"IFCBUILDINGSTOREY('{gid()}',$,'{n}',$,$,{p},$,$,.ELEMENT.,{i*H:.1f})")
    storeys.append((s,i*H,[]))
blocks_storeys=storeys
w.add(f"IFCRELAGGREGATES('{gid()}',$,$,$,{c['bld']},({','.join(s[0] for s in storeys)}))")
ducts=[];pipes=[];trays=[];terms=[]
for bk in range(BLOCKS):
    OX[0]=bk*30000.0
    for si,(st,z,elems) in enumerate(storeys):
        # main duct along X; on levels 0 and 2 it runs into the beams (clashes)
        dz = z+H-250-450 if si!=1 else z+H-250-900   # on level 1 the duct is correctly lowered
        duct=w.add(f"IFCDUCTSEGMENT('{gid()}',$,'Supply Duct SA-{si}01 600x400',$,$,{place(w,c,500,3750,dz)},{box(w,c,23000,600,400,11500,0)},$,.RIGIDSEGMENT.)")
        elems.append(duct); ducts.append(duct)
        # branches along Y crossing the grid-B beams (clash on every level)
        for k,x in enumerate([3000,9000,15000,21000]):
            br=w.add(f"IFCDUCTSEGMENT('{gid()}',$,'Duct Branch SA-{si}0{k+2} 300x250',$,$,{place(w,c,x,3750,dz+75)},{box(w,c,300,7000,250,0,3500)},$,.RIGIDSEGMENT.)")
            elems.append(br); ducts.append(br)
            tm=w.add(f"IFCAIRTERMINAL('{gid()}',$,'Supply Grille G{si}{k+1}',$,$,{place(w,c,x,10000,dz-60)},{box(w,c,600,600,60,0,0)},$,.GRILLE.)"); elems.append(tm); terms.append(tm)
        # heating pipe (swept disk) on grid A, very close to the columns (clearance)
        a=pt(w,0,520,z+2200); b=pt(w,LX,520,z+2200)
        pl=w.add(f"IFCPOLYLINE(({a},{b}))")
        sd=w.add(f"IFCSWEPTDISKSOLID({pl},55.,$,$,$)")
        pp=w.add(f"IFCPIPESEGMENT('{gid()}',$,'Heating Pipe DN100 L{si}',$,$,{place(w,c,0,0,0)},{shape(w,c,sd,'AdvancedSweptSolid')},$,.RIGIDSEGMENT.)"); elems.append(pp); pipes.append(pp)
        # cable tray on grid C
        tr=w.add(f"IFCCABLECARRIERSEGMENT('{gid()}',$,'Cable Tray 300x60 L{si}',$,$,{place(w,c,0,14000,z+2600)},{box(w,c,LX,300,60,LX/2,0)},$,.CABLETRAYSEGMENT.)"); elems.append(tr); trays.append(tr)
OX[0]=0.0
for st,z,elems in storeys:
    w.add(f"IFCRELCONTAINEDINSPATIALSTRUCTURE('{gid()}',$,$,$,({','.join(elems)}),{st})")
pset(w,ducts+terms,'Pset_MEP',{'System':'Supply Air','Phase':'HVAC'})
pset(w,pipes,'Pset_MEP',{'System':'Heating','Phase':'Plumbing'})
pset(w,trays,'Pset_MEP',{'System':'Power','Phase':'Electrical'})
open(os.path.join(out,'Block_A_MEP.ifc'),'w').write(header('Block_A_MEP.ifc','ReferenceView')+"\n".join(w.lines)+"\nENDSEC;\nEND-ISO-10303-21;\n")

if BLOCKS==1:
    open(os.path.join(out,'Block_A_schedule_4D.csv'),'w').write("""Name;Start;End;Type;Rule
Foundations and ground slab;2027-03-01;2027-03-21;Construct;Level=Ground Floor;IFC Type=IfcSlab
Structure ground floor;2027-03-22;2027-04-18;Construct;Level=Ground Floor;Phase=Structure
Slab level 1;2027-04-19;2027-05-02;Construct;Level=Level 1;IFC Type=IfcSlab
Structure level 1;2027-05-03;2027-05-30;Construct;Level=Level 1;Phase=Structure
Slab level 2;2027-05-31;2027-06-13;Construct;Level=Level 2;IFC Type=IfcSlab
Structure level 2;2027-06-14;2027-07-11;Construct;Level=Level 2;Phase=Structure
Roof;2027-07-12;2027-08-01;Construct;IFC Type=IfcRoof
Temporary stair access;2027-06-01;2027-09-15;Temporary;IFC Type=IfcStairFlight
Masonry ground floor;2027-04-19;2027-05-16;Construct;Level=Ground Floor;IFC Type=IfcWall
Masonry level 1;2027-05-31;2027-06-27;Construct;Level=Level 1;IFC Type=IfcWall
Masonry level 2;2027-07-12;2027-08-08;Construct;Level=Level 2;IFC Type=IfcWall
Windows;2027-08-09;2027-09-05;Construct;IFC Type=IfcWindow
Entrance door;2027-08-23;2027-08-29;Construct;IFC Type=IfcDoor
Ventilation;2027-08-09;2027-09-19;Construct;System=Supply Air
Heating;2027-08-16;2027-09-19;Construct;System=Heating
Electrical;2027-08-23;2027-09-26;Construct;System=Power
""")
print("ok")
