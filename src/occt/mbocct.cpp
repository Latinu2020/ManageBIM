// mbocct - shim C pentru OpenCascade (OCCT, LGPL-2.1 + exceptie), folosit de ManageBIM.
//
// Citeste STEP / IGES (prin XCAF: structura de ansamblu, nume, culori) si BREP, trianguleaza
// (BRepMesh_IncrementalMesh) si expune rezultatul printr-un API C simplu: o lista de "piese"
// (instante frunza din ansamblu), fiecare cu triunghiuri in METRI (9 double / triunghi) si o
// culoare RGBA per triunghi. Rust incarca DLL-ul la rulare (LoadLibrary), deci ManageBIM
// porneste si fara OCCT; legarea dinamica pastreaza conditiile LGPL.
//
// Toate sirurile sunt UTF-8. Pointerii intorsi raman valabili pana la mbo_free().

#include <BRepBndLib.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <Bnd_Box.hxx>
#include <IGESCAFControl_Reader.hxx>
#include <IGESControl_Controller.hxx>
#include <IGESData_GlobalSection.hxx>
#include <IGESData_IGESModel.hxx>
#include <IMeshTools_Parameters.hxx>
#include <Interface_Static.hxx>
#include <Message.hxx>
#include <Message_Messenger.hxx>
#include <Message_ProgressIndicator.hxx>
#include <Message_ProgressScope.hxx>
#include <RWMesh_FaceIterator.hxx>
#include <STEPCAFControl_Reader.hxx>
#include <STEPControl_Reader.hxx>
#include <Standard_Failure.hxx>
#include <Standard_Version.hxx>
#include <TDF_Tool.hxx>
#include <TDataStd_Name.hxx>
#include <TDocStd_Document.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Shape.hxx>
#include <XCAFApp_Application.hxx>
#include <XCAFDoc_DocumentTool.hxx>
#include <XCAFDoc_ShapeTool.hxx>
#include <XCAFPrs_DocumentExplorer.hxx>
#include <XCAFPrs_Style.hxx>
// traducator ACIS (descriere B-rep neutra din Rust -> forme OCCT)
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepGProp.hxx>
#include <BRep_Tool.hxx>
#include <ElCLib.hxx>
#include <GProp_GProps.hxx>
#include <Precision.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomConvert.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Circle.hxx>
#include <Geom_ConicalSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_Ellipse.hxx>
#include <Geom_Line.hxx>
#include <Geom_Plane.hxx>
#include <Geom_SphericalSurface.hxx>
#include <Geom_SurfaceOfLinearExtrusion.hxx>
#include <Geom_ToroidalSurface.hxx>
#include <Poly_Triangulation.hxx>
#include <ShapeAnalysis_Curve.hxx>
#include <ShapeAnalysis_Shell.hxx>
#include <ShapeFix_Shape.hxx>
#include <ShapeFix_Solid.hxx>
#include <TColStd_Array1OfInteger.hxx>
#include <TColStd_Array1OfReal.hxx>
#include <TColStd_Array2OfReal.hxx>
#include <TColgp_Array1OfPnt.hxx>
#include <TColgp_Array2OfPnt.hxx>
#include <TopExp.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>

#include <algorithm>
#include <cmath>
#include <exception>
#include <map>
#include <memory>
#include <mutex>
#include <set>
#include <string>
#include <unordered_map>
#include <vector>

#define MBO_API extern "C" __declspec(dllexport)

typedef void (*mbo_progress_fn)(void* ctx, double frac, const char* msg);

namespace
{
struct Part
{
  std::string                name, part_name, path, id, parent, shape_type;
  int                        solids = 0, faces = 0;
  std::vector<double>        xyz;  // 9 per triunghi, metri
  std::vector<unsigned char> rgba; // 4 per triunghi
};
} // namespace

struct MboModel
{
  std::string                                      error;
  std::vector<std::pair<std::string, std::string>> info;
  std::vector<std::string>                         warnings;
  std::vector<Part>                                parts;
};

namespace
{
struct Progress
{
  mbo_progress_fn cb;
  void*           ctx;
  void            operator()(double f, const char* msg) const
  {
    if (cb)
      cb(ctx, f, msg);
  }
};

// Message_ProgressIndicator -> callback-ul din Rust, mapat pe intervalul [lo, hi]
class CbIndicator : public Message_ProgressIndicator
{
public:
  CbIndicator(const Progress& p, double lo, double hi, const char* msg)
      : myP(p), myLo(lo), myHi(hi), myMsg(msg), myLast(-1.0)
  {
  }
  void Show(const Message_ProgressScope&, const bool isForce) override
  {
    double f = GetPosition();
    if (isForce || f - myLast >= 0.01)
    {
      myLast = f;
      myP(myLo + (myHi - myLo) * f, myMsg);
    }
  }

private:
  Progress    myP;
  double      myLo, myHi;
  const char* myMsg;
  double      myLast;
};

std::string utf8(const TCollection_ExtendedString& s)
{
  TCollection_AsciiString a(s); // theReplaceNonAscii = 0 -> UTF-8
  return std::string(a.ToCString());
}

std::string label_name(const TDF_Label& l)
{
  if (l.IsNull())
    return std::string();
  Handle(TDataStd_Name) n;
  if (l.FindAttribute(TDataStd_Name::GetID(), n))
    return utf8(n->Get());
  return std::string();
}

const char* shape_type_name(const TopoDS_Shape& s)
{
  if (s.IsNull())
    return "Empty";
  switch (s.ShapeType())
  {
    case TopAbs_COMPOUND:  return "Compound";
    case TopAbs_COMPSOLID: return "CompSolid";
    case TopAbs_SOLID:     return "Solid";
    case TopAbs_SHELL:     return "Shell";
    case TopAbs_FACE:      return "Face";
    case TopAbs_WIRE:      return "Wire";
    case TopAbs_EDGE:      return "Edge";
    case TopAbs_VERTEX:    return "Vertex";
    default:               return "Shape";
  }
}

int count_of(const TopoDS_Shape& s, TopAbs_ShapeEnum t)
{
  int n = 0;
  for (TopExp_Explorer e(s, t); e.More(); e.Next())
    ++n;
  return n;
}

unsigned char to_u8(double v)
{
  double c = std::round(std::clamp(v, 0.0, 1.0) * 255.0);
  return (unsigned char)c;
}

// sRGB din culoarea liniara OCCT (Quantity_Color tine valori liniare; fisierele STEP au sRGB)
void rgba_of(const Quantity_ColorRGBA& c, unsigned char out[4])
{
  double r, g, b;
  c.GetRGB().Values(r, g, b, Quantity_TOC_sRGB);
  out[0] = to_u8(r);
  out[1] = to_u8(g);
  out[2] = to_u8(b);
  out[3] = to_u8(c.Alpha());
}

std::string join_units(const NCollection_Sequence<TCollection_AsciiString>& s)
{
  std::set<std::string> u;
  for (NCollection_Sequence<TCollection_AsciiString>::Iterator it(s); it.More(); it.Next())
    u.insert(it.Value().ToCString());
  std::string r;
  for (const auto& x : u)
  {
    if (!r.empty())
      r += ", ";
    r += x;
  }
  return r;
}

// Triangulare: fiecare forma unica (piesa referita) o singura data, cu deflexia legata de marimea
// piesei (fina pentru suruburi, rezonabila pentru cadre mari), plafonata de marimea intregului model.
void mesh_document(const Handle(TDocStd_Document)& doc, double rel_defl, double ang_defl, const Progress& prog,
                   MboModel& m)
{
  Handle(XCAFDoc_ShapeTool) st = XCAFDoc_DocumentTool::ShapeTool(doc->Main());
  NCollection_Sequence<TDF_Label> roots;
  st->GetFreeShapes(roots);
  Bnd_Box all;
  for (NCollection_Sequence<TDF_Label>::Iterator it(roots); it.More(); it.Next())
    BRepBndLib::Add(XCAFDoc_ShapeTool::GetShape(it.Value()), all);
  double model_diag = all.IsVoid() ? 1000.0 : std::sqrt(all.SquareExtent());

  // forme unice ale frunzelor (piesele instantiate de mai multe ori se trianguleaza o data)
  std::vector<TopoDS_Shape>                          uniq;
  std::set<const void*>                              seen;
  for (XCAFPrs_DocumentExplorer ex(doc, XCAFPrs_DocumentExplorerFlags_OnlyLeafNodes | XCAFPrs_DocumentExplorerFlags_NoStyle);
       ex.More(); ex.Next())
  {
    TopoDS_Shape s = XCAFDoc_ShapeTool::GetShape(ex.Current().RefLabel);
    if (s.IsNull() || s.TShape().IsNull())
      continue;
    if (seen.insert(s.TShape().get()).second)
      uniq.push_back(s);
  }
  for (size_t i = 0; i < uniq.size(); ++i)
  {
    const TopoDS_Shape& s = uniq[i];
    Bnd_Box b;
    BRepBndLib::Add(s, b);
    double d = b.IsVoid() ? model_diag : std::sqrt(b.SquareExtent());
    IMeshTools_Parameters p;
    p.Deflection   = std::max(std::min(d * rel_defl, model_diag * rel_defl * 0.5), 1e-4);
    p.Angle        = ang_defl;
    p.Relative     = false;
    p.InParallel   = true;
    p.MinSize      = p.Deflection * 0.1;
    p.CleanModel   = true;
    try
    {
      BRepMesh_IncrementalMesh mesher(s, p);
    }
    catch (const Standard_Failure& e)
    {
      m.warnings.push_back(std::string("Tessellation failed for one part: ") + e.what());
    }
    if (i % 16 == 0 || i + 1 == uniq.size())
    {
      char msg[96];
      snprintf(msg, sizeof msg, "Tessellating parts (%zu / %zu)", i + 1, uniq.size());
      prog(0.6 + 0.3 * double(i + 1) / double(uniq.size()), msg);
    }
  }
}

// Frunzele documentului -> piese ManageBIM (pozitie globala, stil mostenit din ansamblu)
void collect_parts(const Handle(TDocStd_Document)& doc, double scale, const Progress& prog, MboModel& m)
{
  prog(0.92, "Collecting parts");
  int no_mesh = 0;
  XCAFPrs_Style def;
  def.SetColorSurf(Quantity_ColorRGBA(Quantity_Color(0.75, 0.75, 0.75, Quantity_TOC_sRGB), 1.0f));
  for (XCAFPrs_DocumentExplorer ex(doc, XCAFPrs_DocumentExplorerFlags_OnlyLeafNodes, def); ex.More(); ex.Next())
  {
    const XCAFPrs_DocumentNode& node = ex.Current();
    Part p;
    p.part_name   = label_name(node.RefLabel);
    std::string inst = label_name(node.Label);
    p.name        = !inst.empty() ? inst : p.part_name;
    p.id          = node.Id.ToCString();
    // calea de ansamblu: numele parintilor (adancimile 0 .. curent-1)
    std::string path;
    int         depth = ex.CurrentDepth();
    for (int d = 0; d < depth; ++d)
    {
      const XCAFPrs_DocumentNode& a = ex.Current(d);
      std::string n = label_name(a.Label);
      if (n.empty())
        n = label_name(a.RefLabel);
      if (n.empty())
        n = "?";
      if (!path.empty())
        path += " / ";
      path += n;
      if (d == depth - 1)
        p.parent = n;
    }
    p.path = path;
    TopoDS_Shape shape = XCAFDoc_ShapeTool::GetShape(node.RefLabel);
    p.shape_type = shape_type_name(shape);
    p.solids     = count_of(shape, TopAbs_SOLID);
    p.faces      = count_of(shape, TopAbs_FACE);

    unsigned char base[4];
    rgba_of(node.Style.IsSetColorSurf() ? node.Style.GetColorSurfRGBA() : def.GetColorSurfRGBA(), base);
    for (RWMesh_FaceIterator fi(node.RefLabel, node.Location, true, node.Style); fi.More(); fi.Next())
    {
      if (fi.IsEmptyMesh())
        continue;
      unsigned char c[4] = {base[0], base[1], base[2], base[3]};
      if (fi.HasFaceColor())
        rgba_of(fi.FaceColor(), c);
      const int lo = fi.ElemLower(), hi = fi.ElemUpper();
      for (int t = lo; t <= hi; ++t)
      {
        Poly_Triangle tri = fi.TriangleOriented(t);
        int           n[3];
        tri.Get(n[0], n[1], n[2]);
        for (int k = 0; k < 3; ++k)
        {
          gp_Pnt q = fi.NodeTransformed(n[k]);
          p.xyz.push_back(q.X() * scale);
          p.xyz.push_back(q.Y() * scale);
          p.xyz.push_back(q.Z() * scale);
        }
        p.rgba.insert(p.rgba.end(), c, c + 4);
      }
    }
    if (p.xyz.empty())
    {
      ++no_mesh; // curbe / puncte fara suprafete
      continue;
    }
    if (p.name.empty())
      p.name = std::string(p.shape_type) + " " + std::to_string(m.parts.size() + 1);
    m.parts.push_back(std::move(p));
  }
  if (no_mesh > 0)
    m.warnings.push_back(std::to_string(no_mesh) + " items without surfaces (curves / points) skipped.");
}

Handle(TDocStd_Document) new_doc()
{
  Handle(XCAFApp_Application) app = XCAFApp_Application::GetApplication();
  Handle(TDocStd_Document)    doc;
  app->NewDocument("MDTV-XCAF", doc);
  // unitatea interna a documentului: milimetri (cititoarele convertesc din unitatea fisierului)
  XCAFDoc_DocumentTool::SetLengthUnit(doc, 1.0, UnitsMethods_LengthUnit_Millimeter);
  return doc;
}

void close_doc(Handle(TDocStd_Document)& doc)
{
  if (doc.IsNull())
    return;
  try
  {
    Handle(XCAFApp_Application) app = XCAFApp_Application::GetApplication();
    if (doc->CanClose() == CDM_CCS_OK)
      app->Close(doc);
  }
  catch (...)
  {
  }
  doc.Nullify();
}

void silence_occt()
{
  static std::once_flag once;
  std::call_once(once, [] {
    // fara mesaje OCCT pe consola (ManageBIM e aplicatie GUI)
    const Handle(Message_Messenger)& msgr = Message::DefaultMessenger();
    NCollection_Sequence<Handle(Message_Printer)> printers = msgr->Printers();
    for (NCollection_Sequence<Handle(Message_Printer)>::Iterator it(printers); it.More(); it.Next())
      msgr->RemovePrinter(it.Value());
  });
}

// ---------------------------------------------------------------- ACIS -> OCCT
// Rust (src/acis.rs) parcurge topologia ACIS (SAT / SAB, parsata de acadrust) si trimite o descriere
// B-rep neutra: doua fluxuri, intregi si reali. Formatul (versiunea 1):
//   ints  : 1 nv nc ne ns nf          reals: tol
//   varfuri  : reals x y z
//   curbe    : int tip; 0 dreapta  -> reals p3 d3
//                       1 elipsa   -> reals centru3 normala3 axa_mare3 raport
//                       2 bspline  -> ints grad nk np rational; reals nk noduri (plate), np * (x y z w)
//                       altceva    -> fara date (curba lipsa -> segment intre varfuri)
//   muchii   : ints v_start v_end curba(-1) inversata
//   suprafete: ints tip flip; 0 plan    -> reals origine3 normala3 u3
//                             1 con     -> reals centru3 axa3 axa_mare3 raport sin cos
//                             2 sfera   -> reals centru3 raza u3 pol3
//                             3 tor     -> reals centru3 normala3 R r u3
//                             4 bspline -> ints du dv nku nkv nu nv rational; reals noduri u, noduri v,
//                                          nu*nv * (x y z w), index v*nu+u
//                             altceva   -> nesuportata (fata sarita)
//   fete     : ints suprafata inversata shell nbucle; pe bucla: ints ncoedge; pe coedge: ints muchie inversata
// "flip" = normala suprafetei OCCT e opusa celei ACIS (con cu cos < 0, sfera / tor cu raza negativa,
// spline "reversed"); inversata = sensul fetei / coedge-ului fata de suprafata / muchie.
struct DescReader
{
  const int*    I;
  int           ni, ip = 0;
  const double* R;
  int           nr, rp = 0;
  bool          bad = false;
  int           i()
  {
    if (ip >= ni)
    {
      bad = true;
      return 0;
    }
    return I[ip++];
  }
  double r()
  {
    if (rp >= nr)
    {
      bad = true;
      return 0.0;
    }
    return R[rp++];
  }
  gp_Pnt pnt()
  {
    double x = r(), y = r(), z = r();
    return gp_Pnt(x, y, z);
  }
  gp_Vec vec()
  {
    double x = r(), y = r(), z = r();
    return gp_Vec(x, y, z);
  }
};

// vector de noduri plat -> noduri distincte + multiplicitati
void split_knots(const std::vector<double>& flat, TColStd_Array1OfReal*& K, TColStd_Array1OfInteger*& M)
{
  std::vector<double> kv;
  std::vector<int>    km;
  for (double k : flat)
  {
    if (!kv.empty() && std::abs(k - kv.back()) <= 1e-12 * (1.0 + std::abs(k)))
      km.back()++;
    else
    {
      kv.push_back(k);
      km.push_back(1);
    }
  }
  K = new TColStd_Array1OfReal(1, (int)kv.size());
  M = new TColStd_Array1OfInteger(1, (int)km.size());
  for (size_t j = 0; j < kv.size(); ++j)
  {
    K->SetValue((int)j + 1, kv[j]);
    M->SetValue((int)j + 1, km[j]);
  }
}

Handle(Geom_Curve) ellipse_curve(const gp_Pnt& c, const gp_Vec& n, const gp_Vec& maj, double ratio)
{
  double R = maj.Magnitude(), r = R * std::abs(ratio);
  if (std::abs(r - R) <= 1e-9 * R)
    return new Geom_Circle(gp_Ax2(c, gp_Dir(n), gp_Dir(maj)), R);
  if (r < R)
    return new Geom_Ellipse(gp_Ax2(c, gp_Dir(n), gp_Dir(maj)), R, r);
  // raport > 1: axa mare devine cea perpendiculara
  gp_Dir y = gp_Dir(n).Crossed(gp_Dir(maj));
  return new Geom_Ellipse(gp_Ax2(c, gp_Dir(n), y), r, R);
}

double param_on(const Handle(Geom_Curve)& c, const gp_Pnt& p, double tol)
{
  if (Handle(Geom_Line) l = Handle(Geom_Line)::DownCast(c))
    return ElCLib::Parameter(l->Lin(), p);
  if (Handle(Geom_Circle) ci = Handle(Geom_Circle)::DownCast(c))
    return ElCLib::Parameter(ci->Circ(), p);
  if (Handle(Geom_Ellipse) e = Handle(Geom_Ellipse)::DownCast(c))
    return ElCLib::Parameter(e->Elips(), p);
  ShapeAnalysis_Curve sac;
  gp_Pnt              pr;
  double              u = c->FirstParameter();
  sac.Project(c, p, tol, pr, u, false);
  return u;
}

// muchie p1 -> p2 pe curba (inversata daca muchia ACIS e "reversed"); nula = muchie degenerata
TopoDS_Edge make_edge(Handle(Geom_Curve) c, bool rev, const gp_Pnt& p1, const gp_Pnt& p2, const TopoDS_Vertex& v1,
                      const TopoDS_Vertex& v2, bool same_vertex, double tol, bool& fallback)
{
  if (c.IsNull())
  {
    if (same_vertex || p1.Distance(p2) <= tol)
      return TopoDS_Edge();
    c        = new Geom_Line(p1, gp_Dir(gp_Vec(p1, p2)));
    rev      = false;
    fallback = true;
  }
  else if (rev)
    c = c->Reversed();
  double u1 = param_on(c, p1, tol), u2;
  if (same_vertex)
  {
    if (c->IsPeriodic())
      u2 = u1 + c->Period();
    else
    {
      u1 = c->FirstParameter();
      u2 = c->LastParameter();
    }
  }
  else
  {
    u2 = param_on(c, p2, tol);
    if (c->IsPeriodic())
    {
      const double T = c->Period();
      while (u2 <= u1 + 1e-12)
        u2 += T;
      while (u2 - u1 > T)
        u2 -= T;
    }
    else if (u2 < u1)
    {
      // curba merge invers fata de muchie (sens ACIS neobisnuit): o intoarcem
      c  = c->Reversed();
      u1 = param_on(c, p1, tol);
      u2 = param_on(c, p2, tol);
    }
  }
  if (!(u2 > u1) || Precision::IsInfinite(u1) || Precision::IsInfinite(u2))
    return TopoDS_Edge();
  BRep_Builder B;
  TopoDS_Edge  E;
  B.MakeEdge(E, c, tol);
  B.Add(E, v1.Oriented(TopAbs_FORWARD));
  B.Add(E, v2.Oriented(TopAbs_REVERSED));
  B.Range(E, u1, u2);
  return E;
}

struct SurfDesc
{
  Handle(Geom_Surface) s, alt; // alt: aceeasi retea BSpline citita transpus (ordinea punctelor nesigura)
  bool                 flip = false;
  bool                 chosen = false;
  // con eliptic (OCCT nu are unul analitic): construit la prima fata, din conul circular in limitele ei
  bool   ell_cone = false;
  gp_Ax3 ax;
  double semi = 0.0, R = 0.0, ratio = 1.0;
};

// Conul eliptic ACIS = imaginea afina a conului circular (scalare pe axa mica cu raportul). Conul circular,
// limitat axial la varfurile fetei (+ margine), devine BSpline; scalarea polilor e exacta (NURBS e invariant afin).
Handle(Geom_Surface) elliptic_cone(const SurfDesc& sd, const std::vector<gp_Pnt>& pts)
{
  if (pts.empty())
    return Handle(Geom_Surface)();
  const gp_Vec axis(sd.ax.Direction());
  double       hmin = 1e300, hmax = -1e300;
  for (const gp_Pnt& p : pts)
  {
    const double h = gp_Vec(sd.ax.Location(), p).Dot(axis);
    hmin           = std::min(hmin, h);
    hmax           = std::max(hmax, h);
  }
  const double pad = std::max(0.05 * (hmax - hmin), 0.01 * sd.R);
  const double c = std::cos(sd.semi), s = std::sin(sd.semi);
  double       v0 = (hmin - pad) / c, v1 = (hmax + pad) / c;
  // nu trecem de varf (raza R + v sin >= 0)
  if (std::abs(s) > 1e-12)
  {
    const double apex = -sd.R / s;
    if (s > 0.0)
      v0 = std::max(v0, apex);
    else
      v1 = std::min(v1, apex);
  }
  if (!(v1 > v0))
    return Handle(Geom_Surface)();
  Handle(Geom_ConicalSurface)          cone = new Geom_ConicalSurface(sd.ax, sd.semi, sd.R);
  Handle(Geom_RectangularTrimmedSurface) tr = new Geom_RectangularTrimmedSurface(cone, 0.0, 2.0 * 3.14159265358979323846, v0, v1);
  Handle(Geom_BSplineSurface)          bs   = GeomConvert::SurfaceToBSplineSurface(tr);
  const gp_Pnt                         O    = sd.ax.Location();
  const gp_Vec                         Y(sd.ax.YDirection());
  for (int i = 1; i <= bs->NbUPoles(); ++i)
    for (int j = 1; j <= bs->NbVPoles(); ++j)
    {
      gp_Pnt       P = bs->Pole(i, j);
      const double y = gp_Vec(O, P).Dot(Y);
      P.Translate(Y * (y * (sd.ratio - 1.0)));
      bs->SetPole(i, j, P);
    }
  return bs;
}

double max_dist(const Handle(Geom_Surface)& s, const std::vector<gp_Pnt>& pts)
{
  double d = 0.0;
  for (const gp_Pnt& p : pts)
  {
    GeomAPI_ProjectPointOnSurf pr(p, s);
    d = std::max(d, pr.NbPoints() > 0 ? pr.LowerDistance() : 1e300);
  }
  return d;
}

void tris_of(const TopoDS_Shape& S, Part& part, int& no_mesh)
{
  for (TopExp_Explorer ex(S, TopAbs_FACE); ex.More(); ex.Next())
  {
    const TopoDS_Face&         F = TopoDS::Face(ex.Current());
    TopLoc_Location            loc;
    Handle(Poly_Triangulation) T = BRep_Tool::Triangulation(F, loc);
    if (T.IsNull() || T->NbTriangles() == 0)
    {
      ++no_mesh;
      continue;
    }
    const gp_Trsf tr  = loc.Transformation();
    const bool    rev = F.Orientation() == TopAbs_REVERSED;
    for (int t = 1; t <= T->NbTriangles(); ++t)
    {
      int a, b, c;
      T->Triangle(t).Get(a, b, c);
      if (rev)
        std::swap(b, c);
      for (int n : {a, b, c})
      {
        gp_Pnt q = T->Node(n).Transformed(tr);
        part.xyz.push_back(q.X());
        part.xyz.push_back(q.Y());
        part.xyz.push_back(q.Z());
      }
      static const unsigned char grey[4] = {192, 192, 192, 255};
      part.rgba.insert(part.rgba.end(), grey, grey + 4);
    }
  }
}
} // namespace

MBO_API int mbo_version()
{
  return OCC_VERSION_HEX;
}

// kind: 0 = STEP, 1 = IGES, 2 = BREP. rel_defl: deflexia liniara ca fractie din marimea piesei
// (ex. 0.002), ang_defl: deflexia unghiulara in radiani (ex. 0.35).
MBO_API MboModel* mbo_load(const char* path, int kind, double rel_defl, double ang_defl, mbo_progress_fn cb, void* ctx)
{
  // XCAFApp_Application e un singleton: o singura incarcare OCCT odata (modelele se pot deschide in paralel)
  static std::mutex           lock;
  std::lock_guard<std::mutex> guard(lock);
  MboModel* m = new MboModel();
  Progress  prog{cb, ctx};
  Handle(TDocStd_Document) doc;
  try
  {
    silence_occt();
    if (rel_defl <= 0.0)
      rel_defl = 0.002;
    if (ang_defl <= 0.0)
      ang_defl = 0.35;
    doc = new_doc();
    double scale = 0.001; // mm -> m
    if (kind == 0)
    {
      prog(0.02, "Reading STEP file");
      STEPCAFControl_Reader r;
      r.SetColorMode(true);
      r.SetNameMode(true);
      r.SetLayerMode(false);
      r.SetPropsMode(false);
      r.SetGDTMode(false);
      r.SetMatMode(false);
      r.SetViewMode(false);
      if (r.ReadFile(path) != IFSelect_RetDone)
      {
        m->error = "Cannot read the STEP file (unsupported or damaged).";
        close_doc(doc);
        return m;
      }
      NCollection_Sequence<TCollection_AsciiString> lu, au, su;
      try
      {
        r.ChangeReader().FileUnits(lu, au, su);
        std::string u = join_units(lu);
        if (!u.empty())
          m->info.push_back({"File units", u});
      }
      catch (...)
      {
      }
      Handle(CbIndicator) ind = new CbIndicator(prog, 0.3, 0.6, "Translating STEP entities");
      prog(0.3, "Translating STEP entities");
      if (!r.Transfer(doc, ind->Start()))
      {
        m->error = "The STEP file contains no transferable geometry.";
        close_doc(doc);
        return m;
      }
      m->info.push_back({"Format", "STEP (ISO 10303-21)"});
    }
    else if (kind == 1)
    {
      prog(0.02, "Reading IGES file");
      IGESControl_Controller::Init();
      IGESCAFControl_Reader r;
      r.SetColorMode(true);
      r.SetNameMode(true);
      r.SetLayerMode(true);
      if (r.ReadFile(path) != IFSelect_RetDone)
      {
        m->error = "Cannot read the IGES file (unsupported or damaged).";
        close_doc(doc);
        return m;
      }
      try
      {
        Handle(IGESData_IGESModel) im = r.IGESModel();
        if (!im.IsNull())
        {
          Handle(TCollection_HAsciiString) un = im->GlobalSection().UnitName();
          if (!un.IsNull())
            m->info.push_back({"File units", un->ToCString()});
        }
      }
      catch (...)
      {
      }
      Handle(CbIndicator) ind = new CbIndicator(prog, 0.3, 0.6, "Translating IGES entities");
      prog(0.3, "Translating IGES entities");
      if (!r.Transfer(doc, ind->Start()))
      {
        m->error = "The IGES file contains no transferable geometry.";
        close_doc(doc);
        return m;
      }
      m->info.push_back({"Format", "IGES"});
    }
    else
    {
      prog(0.02, "Reading BREP file");
      TopoDS_Shape s;
      BRep_Builder b;
      Handle(CbIndicator) ind = new CbIndicator(prog, 0.02, 0.5, "Reading BREP file");
      if (!BRepTools::Read(s, path, b, ind->Start()) || s.IsNull())
      {
        m->error = "Cannot read the BREP file (unsupported or damaged).";
        close_doc(doc);
        return m;
      }
      Handle(XCAFDoc_ShapeTool) st = XCAFDoc_DocumentTool::ShapeTool(doc->Main());
      // compound -> ansamblu: fiecare solid / sub-forma devine o piesa separata
      TDF_Label l = st->AddShape(s, s.ShapeType() == TopAbs_COMPOUND);
      TDataStd_Name::Set(l, TCollection_ExtendedString("BREP"));
      m->info.push_back({"Format", "OpenCascade BREP"});
      m->info.push_back({"File units", "none (assumed mm)"});
    }
    mesh_document(doc, rel_defl, ang_defl, prog, *m);
    collect_parts(doc, scale, prog, *m);
    char ver[64];
    snprintf(ver, sizeof ver, "Open CASCADE %s", OCC_VERSION_COMPLETE);
    m->info.push_back({"Reader", ver});
    if (m->parts.empty() && m->error.empty())
      m->error = "No 3D surfaces in this file (only curves / points, or the shapes could not be tessellated).";
  }
  catch (const Standard_Failure& e)
  {
    m->error = std::string("OpenCascade error: ") + (e.what() ? e.what() : "unknown");
  }
  catch (const std::exception& e)
  {
    m->error = std::string("OpenCascade error: ") + e.what();
  }
  catch (...)
  {
    m->error = "OpenCascade error (unknown exception).";
  }
  close_doc(doc);
  return m;
}

// Corp ACIS descris de Rust (formatul de mai sus) -> fete OCCT, ShapeFix (pcurve-uri, cusaturi, tolerante,
// orientare), solide din shell-urile inchise (orientate spre exterior), triangulare. Rezultatul: o singura
// piesa, in unitatile descrierii (fara transformarea corpului - o aplica Rust). Info: faces, faces_failed,
// solids, open_shells, curve_fallbacks. Fir-sigur (fara XCAF): Rust il poate apela in paralel.
MBO_API MboModel* mbo_brep_from_desc(const int* ints, int nints, const double* reals, int nreals, double rel_defl,
                                     double ang_defl)
{
  MboModel* m = new MboModel();
  try
  {
    silence_occt();
    if (rel_defl <= 0.0)
      rel_defl = 0.002;
    if (ang_defl <= 0.0)
      ang_defl = 0.35;
    DescReader d{ints, nints, 0, reals, nreals, 0};
    if (d.i() != 1)
    {
      m->error = "Unsupported B-rep description version.";
      return m;
    }
    const int nv = d.i(), nc = d.i(), ne = d.i(), ns = d.i(), nf = d.i();
    double    tol = d.r();
    if (d.bad || nv < 0 || nc < 0 || ne < 0 || ns < 0 || nf < 0)
    {
      m->error = "Damaged B-rep description.";
      return m;
    }
    if (!(tol > 0.0))
      tol = 1e-6;
    BRep_Builder B;

    // varfuri
    std::vector<gp_Pnt>        vp(nv);
    std::vector<TopoDS_Vertex> vv(nv);
    for (int k = 0; k < nv; ++k)
    {
      vp[k] = d.pnt();
      B.MakeVertex(vv[k], vp[k], tol);
    }

    // curbe (o curba invalida -> nula, muchiile ei devin segmente)
    std::vector<Handle(Geom_Curve)> cv(nc);
    int                             bad_curves = 0;
    for (int k = 0; k < nc; ++k)
    {
      const int t = d.i();
      try
      {
        if (t == 0)
        {
          gp_Pnt p = d.pnt();
          gp_Vec v = d.vec();
          cv[k]    = new Geom_Line(p, gp_Dir(v));
        }
        else if (t == 1)
        {
          gp_Pnt c   = d.pnt();
          gp_Vec n   = d.vec();
          gp_Vec maj = d.vec();
          double rat = d.r();
          cv[k]      = ellipse_curve(c, n, maj, rat);
        }
        else if (t == 2)
        {
          const int deg = d.i(), nk = d.i(), np = d.i(), rational = d.i();
          std::vector<double> flat(std::max(nk, 0));
          for (double& x : flat)
            x = d.r();
          TColgp_Array1OfPnt   P(1, std::max(np, 1));
          TColStd_Array1OfReal W(1, std::max(np, 1));
          for (int j = 1; j <= np; ++j)
          {
            P(j) = d.pnt();
            W(j) = d.r();
          }
          TColStd_Array1OfReal*    K = nullptr;
          TColStd_Array1OfInteger* M = nullptr;
          split_knots(flat, K, M);
          std::unique_ptr<TColStd_Array1OfReal>    Kp(K);
          std::unique_ptr<TColStd_Array1OfInteger> Mp(M);
          cv[k] = rational ? new Geom_BSplineCurve(P, W, *K, *M, deg) : new Geom_BSplineCurve(P, *K, *M, deg);
        }
      }
      catch (const Standard_Failure&)
      {
        cv[k].Nullify();
        ++bad_curves;
      }
    }
    if (d.bad)
    {
      m->error = "Damaged B-rep description (curves).";
      return m;
    }

    // muchii
    std::vector<TopoDS_Edge> ev(ne);
    int                      fallbacks = 0;
    for (int k = 0; k < ne; ++k)
    {
      const int sv = d.i(), en = d.i(), c = d.i(), rev = d.i();
      if (d.bad || sv < 0 || sv >= nv || en < 0 || en >= nv)
        continue;
      try
      {
        bool fb = false;
        ev[k]   = make_edge(c >= 0 && c < nc ? cv[c] : Handle(Geom_Curve)(), rev != 0, vp[sv], vp[en], vv[sv], vv[en],
                            sv == en, tol, fb);
        if (fb && c >= 0)
          ++fallbacks;
      }
      catch (const Standard_Failure&)
      {
        ev[k].Nullify();
      }
    }

    // suprafete
    std::vector<SurfDesc> sv(ns);
    for (int k = 0; k < ns; ++k)
    {
      const int t    = d.i();
      sv[k].flip     = d.i() != 0;
      try
      {
        if (t == 0)
        {
          gp_Pnt o = d.pnt();
          gp_Vec n = d.vec(), u = d.vec();
          sv[k].s  = new Geom_Plane(gp_Ax3(o, gp_Dir(n), gp_Dir(u)));
        }
        else if (t == 1)
        {
          gp_Pnt c   = d.pnt();
          gp_Vec ax  = d.vec(), maj = d.vec();
          double rat = d.r(), s = d.r(), co = d.r();
          double R   = maj.Magnitude();
          gp_Ax3 a3(c, gp_Dir(ax), gp_Dir(maj));
          // normala ACIS = cos * radial - sin * axa: cos < 0 -> normala spre axa
          if (co < 0.0)
            sv[k].flip = !sv[k].flip;
          if (std::abs(s) < 1e-12)
          {
            if (std::abs(std::abs(rat) - 1.0) < 1e-9)
              sv[k].s = new Geom_CylindricalSurface(a3, R);
            else // cilindru eliptic: extrudarea elipsei de baza
              sv[k].s = new Geom_SurfaceOfLinearExtrusion(ellipse_curve(c, ax, maj, rat), gp_Dir(ax));
          }
          else if (std::abs(std::abs(rat) - 1.0) < 1e-9)
            sv[k].s = new Geom_ConicalSurface(a3, std::atan(s / co), R);
          else
          {
            sv[k].ell_cone = true;
            sv[k].ax       = a3;
            sv[k].semi     = std::atan(s / co);
            sv[k].R        = R;
            sv[k].ratio    = std::abs(rat);
          }
        }
        else if (t == 2)
        {
          gp_Pnt c = d.pnt();
          double R = d.r();
          gp_Vec u = d.vec(), pole = d.vec();
          if (R < 0.0)
            sv[k].flip = !sv[k].flip;
          sv[k].s = new Geom_SphericalSurface(gp_Ax3(c, gp_Dir(pole), gp_Dir(u)), std::abs(R));
        }
        else if (t == 3)
        {
          gp_Pnt c  = d.pnt();
          gp_Vec n  = d.vec();
          double R  = d.r(), r = d.r();
          gp_Vec u  = d.vec();
          if (r < 0.0)
            sv[k].flip = !sv[k].flip;
          if (R > 0.0)
            sv[k].s = new Geom_ToroidalSurface(gp_Ax3(c, gp_Dir(n), gp_Dir(u)), R, std::abs(r));
        }
        else if (t == 4)
        {
          const int du = d.i(), dv = d.i(), nku = d.i(), nkv = d.i(), nu = d.i(), nvv = d.i(), rational = d.i();
          std::vector<double> fu(std::max(nku, 0)), fv(std::max(nkv, 0));
          for (double& x : fu)
            x = d.r();
          for (double& x : fv)
            x = d.r();
          const int           npt = std::max(nu, 0) * std::max(nvv, 0);
          std::vector<double> raw((size_t)npt * 4);
          for (double& x : raw)
            x = d.r();
          if (!d.bad && nu > 1 && nvv > 1)
          {
            TColStd_Array1OfReal *   KU = nullptr, *KV = nullptr;
            TColStd_Array1OfInteger *MU = nullptr, *MV = nullptr;
            split_knots(fu, KU, MU);
            split_knots(fv, KV, MV);
            std::unique_ptr<TColStd_Array1OfReal>    a(KU), b(KV);
            std::unique_ptr<TColStd_Array1OfInteger> c(MU), e(MV);
            auto build = [&](bool transposed) -> Handle(Geom_Surface) {
              TColgp_Array2OfPnt   P(1, nu, 1, nvv);
              TColStd_Array2OfReal W(1, nu, 1, nvv);
              for (int iu = 0; iu < nu; ++iu)
                for (int iv = 0; iv < nvv; ++iv)
                {
                  const size_t j = 4 * (size_t)(transposed ? iu * nvv + iv : iv * nu + iu);
                  P(iu + 1, iv + 1) = gp_Pnt(raw[j], raw[j + 1], raw[j + 2]);
                  W(iu + 1, iv + 1) = raw[j + 3];
                }
              return rational ? new Geom_BSplineSurface(P, W, *KU, *KV, *MU, *MV, du, dv)
                              : new Geom_BSplineSurface(P, *KU, *KV, *MU, *MV, du, dv);
            };
            sv[k].s = build(false);
            try
            {
              sv[k].alt = build(true);
            }
            catch (const Standard_Failure&)
            {
            }
          }
        }
      }
      catch (const Standard_Failure&)
      {
        sv[k].s.Nullify();
      }
    }
    if (d.bad)
    {
      m->error = "Damaged B-rep description (surfaces).";
      return m;
    }

    // fete, grupate pe shell-uri
    std::map<int, TopoDS_Shell> shells;
    int                         failed = 0;
    for (int k = 0; k < nf; ++k)
    {
      const int si = d.i(), frev = d.i(), shell = d.i(), nl = d.i();
      std::vector<std::vector<std::pair<int, int>>> loops(std::max(nl, 0));
      for (auto& l : loops)
      {
        const int nce = d.i();
        for (int j = 0; j < nce && !d.bad; ++j)
        {
          const int e = d.i(), r = d.i();
          l.push_back({e, r});
        }
      }
      if (d.bad)
        break;
      if (si < 0 || si >= ns || (sv[si].s.IsNull() && !sv[si].ell_cone))
      {
        ++failed;
        continue;
      }
      try
      {
        SurfDesc&            sd   = sv[si];
        Handle(Geom_Surface) surf = sd.s;
        if (sd.ell_cone)
        {
          std::vector<gp_Pnt> pts;
          for (const auto& l : loops)
            for (const auto& ce : l)
              if (ce.first >= 0 && ce.first < ne && !ev[ce.first].IsNull())
              {
                // capetele si mijlocul muchiei: arcele de baza dau extinderea axiala corecta
                const TopoDS_Edge& E = ev[ce.first];
                double             f, l2;
                Handle(Geom_Curve) c = BRep_Tool::Curve(E, f, l2);
                pts.push_back(BRep_Tool::Pnt(TopExp::FirstVertex(E)));
                if (!c.IsNull())
                  pts.push_back(c->Value(0.5 * (f + l2)));
              }
          // per fata: limitele axiale difera intre fetele aceluiasi con
          surf = elliptic_cone(sd, pts);
          if (surf.IsNull())
          {
            ++failed;
            continue;
          }
        }
        // retea BSpline: alegem ordinea punctelor care trece prin varfurile fetei
        if (!sd.alt.IsNull() && !sd.chosen)
        {
          std::vector<gp_Pnt> pts;
          for (const auto& l : loops)
            for (const auto& ce : l)
              if (ce.first >= 0 && ce.first < ne && !ev[ce.first].IsNull())
                pts.push_back(BRep_Tool::Pnt(TopExp::FirstVertex(ev[ce.first])));
          if (!pts.empty() && max_dist(sd.alt, pts) < 0.5 * max_dist(sd.s, pts))
            sd.s = sd.alt;
          sd.chosen = true;
          surf      = sd.s;
        }
        const bool  rev = (frev != 0) != sd.flip;
        TopoDS_Face F;
        int         wires = 0;
        if (loops.empty())
        {
          // fata fara bucle = suprafata inchisa intreaga (sfera, tor)
          if (Handle(Geom_Plane)::DownCast(surf).IsNull())
          {
            BRepBuilderAPI_MakeFace mf(surf, tol);
            if (mf.IsDone())
            {
              F     = mf.Face();
              wires = 1;
            }
          }
        }
        else
        {
          B.MakeFace(F, surf, tol);
          for (const auto& l : loops)
          {
            TopoDS_Wire W;
            B.MakeWire(W);
            int n = 0;
            for (const auto& ce : l)
            {
              if (ce.first < 0 || ce.first >= ne || ev[ce.first].IsNull())
                continue;
              // muchiile se pastreaza fata de suprafata (OCCT); sensul fetei se aplica la final
              const bool er = (ce.second != 0) != rev;
              B.Add(W, er ? ev[ce.first].Reversed() : ev[ce.first]);
              ++n;
            }
            if (n > 0)
            {
              B.Add(F, W);
              ++wires;
            }
          }
        }
        if (F.IsNull() || wires == 0)
        {
          ++failed;
          continue;
        }
        if (rev)
          F.Orientation(TopAbs_REVERSED);
        auto it = shells.find(shell);
        if (it == shells.end())
        {
          TopoDS_Shell sh;
          B.MakeShell(sh);
          it = shells.emplace(shell, sh).first;
        }
        B.Add(it->second, F);
      }
      catch (const Standard_Failure&)
      {
        ++failed;
      }
    }
    if (d.bad)
    {
      m->error = "Damaged B-rep description (faces).";
      return m;
    }

    TopoDS_Compound raw;
    B.MakeCompound(raw);
    for (auto& s : shells)
      B.Add(raw, s.second);
    // ShapeFix: pcurve-uri, cusaturi pe suprafete periodice, bucle exterioare / gauri, tolerante, orientare
    Bnd_Box bb;
    BRepBndLib::Add(raw, bb);
    const double diag = bb.IsVoid() ? 1.0 : std::sqrt(bb.SquareExtent());
    TopoDS_Shape fixed = raw;
    try
    {
      Handle(ShapeFix_Shape) fix = new ShapeFix_Shape(raw);
      fix->SetPrecision(tol);
      fix->SetMaxTolerance(std::max(diag * 1e-3, tol * 10.0));
      fix->Perform();
      fixed = fix->Shape();
    }
    catch (const Standard_Failure& e)
    {
      m->warnings.push_back(std::string("Shape healing failed: ") + e.what());
    }

    // shell inchis -> solid orientat spre exterior; restul raman suprafete
    TopoDS_Compound res;
    B.MakeCompound(res);
    int solids = 0, open_shells = 0;
    for (TopExp_Explorer ex(fixed, TopAbs_SHELL); ex.More(); ex.Next())
    {
      const TopoDS_Shell& sh = TopoDS::Shell(ex.Current());
      bool                done = false;
      try
      {
        if (BRep_Tool::IsClosed(sh))
        {
          ShapeFix_Solid sfs;
          TopoDS_Solid   so = sfs.SolidFromShell(sh);
          GProp_GProps   g;
          BRepGProp::VolumeProperties(so, g);
          if (g.Mass() < 0.0)
            so.Reverse();
          B.Add(res, so);
          ++solids;
          done = true;
        }
      }
      catch (const Standard_Failure&)
      {
      }
      if (!done)
      {
        B.Add(res, sh);
        ++open_shells;
      }
    }
    for (TopExp_Explorer ex(fixed, TopAbs_FACE, TopAbs_SHELL); ex.More(); ex.Next())
      B.Add(res, ex.Current());

    IMeshTools_Parameters p;
    p.Deflection = std::max(diag * rel_defl, 1e-9);
    p.Angle      = ang_defl;
    p.Relative   = false;
    p.InParallel = false;
    p.MinSize    = p.Deflection * 0.1;
    try
    {
      BRepMesh_IncrementalMesh mesher(res, p);
    }
    catch (const Standard_Failure& e)
    {
      m->warnings.push_back(std::string("Tessellation failed: ") + e.what());
    }
    Part part;
    int  no_mesh = 0;
    tris_of(res, part, no_mesh);
    part.solids = solids;
    part.faces  = count_of(res, TopAbs_FACE);
    part.name   = "ACIS body";
    m->info.push_back({"faces", std::to_string(nf)});
    m->info.push_back({"faces_failed", std::to_string(failed + no_mesh)});
    m->info.push_back({"solids", std::to_string(solids)});
    m->info.push_back({"open_shells", std::to_string(open_shells)});
    m->info.push_back({"curve_fallbacks", std::to_string(fallbacks + bad_curves)});
    if (!part.xyz.empty())
      m->parts.push_back(std::move(part));
  }
  catch (const Standard_Failure& e)
  {
    m->error = std::string("OpenCascade error: ") + (e.what() ? e.what() : "unknown");
  }
  catch (const std::exception& e)
  {
    m->error = std::string("OpenCascade error: ") + e.what();
  }
  catch (...)
  {
    m->error = "OpenCascade error (unknown exception).";
  }
  return m;
}

MBO_API const char* mbo_error(const MboModel* m)
{
  return m ? m->error.c_str() : "null model";
}

MBO_API int mbo_info_count(const MboModel* m)
{
  return m ? (int)m->info.size() : 0;
}

MBO_API const char* mbo_info(const MboModel* m, int i, int value)
{
  if (!m || i < 0 || i >= (int)m->info.size())
    return "";
  return value ? m->info[i].second.c_str() : m->info[i].first.c_str();
}

MBO_API int mbo_warning_count(const MboModel* m)
{
  return m ? (int)m->warnings.size() : 0;
}

MBO_API const char* mbo_warning(const MboModel* m, int i)
{
  if (!m || i < 0 || i >= (int)m->warnings.size())
    return "";
  return m->warnings[i].c_str();
}

MBO_API int mbo_part_count(const MboModel* m)
{
  return m ? (int)m->parts.size() : 0;
}

// field: 0 nume afisat, 1 nume piesa (definitie), 2 cale ansamblu, 3 id (cale XCAF), 4 parinte, 5 tip forma
MBO_API const char* mbo_part_str(const MboModel* m, int i, int field)
{
  if (!m || i < 0 || i >= (int)m->parts.size())
    return "";
  const Part& p = m->parts[i];
  switch (field)
  {
    case 0: return p.name.c_str();
    case 1: return p.part_name.c_str();
    case 2: return p.path.c_str();
    case 3: return p.id.c_str();
    case 4: return p.parent.c_str();
    case 5: return p.shape_type.c_str();
    default: return "";
  }
}

// field: 0 solide, 1 fete
MBO_API int mbo_part_int(const MboModel* m, int i, int field)
{
  if (!m || i < 0 || i >= (int)m->parts.size())
    return 0;
  return field == 0 ? m->parts[i].solids : m->parts[i].faces;
}

MBO_API int mbo_part_tri_count(const MboModel* m, int i)
{
  if (!m || i < 0 || i >= (int)m->parts.size())
    return 0;
  return (int)(m->parts[i].rgba.size() / 4);
}

MBO_API const double* mbo_part_xyz(const MboModel* m, int i)
{
  if (!m || i < 0 || i >= (int)m->parts.size() || m->parts[i].xyz.empty())
    return nullptr;
  return m->parts[i].xyz.data();
}

MBO_API const unsigned char* mbo_part_rgba(const MboModel* m, int i)
{
  if (!m || i < 0 || i >= (int)m->parts.size() || m->parts[i].rgba.empty())
    return nullptr;
  return m->parts[i].rgba.data();
}

MBO_API void mbo_free(MboModel* m)
{
  delete m;
}
