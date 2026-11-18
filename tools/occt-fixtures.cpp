// Genereaza fisierele de test pentru cititorul OpenCascade (tests/fixtures/cad/):
//   asm.step    ansamblu STEP AP214 (mm): placa rosie cu fata de sus galbena, 2 instante "Pin" albastre,
//               sub-ansamblu cu un con verde
//   asm_m.step  o cutie 2 x 1 x 0.5 m scrisa in METRI (test conversie unitati)
//   asm.igs     aceleasi piese in IGES
//   parts.brep  compound BREP: cutie + sfera
// Compilare (o singura data, din "x64 Native Tools Command Prompt"):
//   cl /nologo /EHsc /MD /std:c++17 /I third_party\occt\inc tools\occt-fixtures.cpp /link /LIBPATH:<lib> TK*.lib
#include <BRepBndLib.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <Bnd_Box.hxx>
#include <IGESCAFControl_Writer.hxx>
#include <IGESControl_Controller.hxx>
#include <Interface_Static.hxx>
#include <STEPCAFControl_Controller.hxx>
#include <STEPCAFControl_Writer.hxx>
#include <TDataStd_Name.hxx>
#include <TDocStd_Document.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <XCAFApp_Application.hxx>
#include <XCAFDoc_ColorTool.hxx>
#include <XCAFDoc_DocumentTool.hxx>
#include <XCAFDoc_ShapeTool.hxx>
#include <cstdio>
#include <string>

static TopLoc_Location at(double x, double y, double z)
{
  gp_Trsf t;
  t.SetTranslation(gp_Vec(x, y, z));
  return TopLoc_Location(t);
}

static Quantity_Color rgb(double r, double g, double b)
{
  return Quantity_Color(r, g, b, Quantity_TOC_sRGB);
}

static Handle(TDocStd_Document) make_doc(UnitsMethods_LengthUnit unit)
{
  Handle(TDocStd_Document) doc;
  XCAFApp_Application::GetApplication()->NewDocument("MDTV-XCAF", doc);
  XCAFDoc_DocumentTool::SetLengthUnit(doc, 1.0, unit);
  return doc;
}

int main(int argc, char** argv)
{
  std::string dir = argc > 1 ? argv[1] : ".";
  STEPCAFControl_Controller::Init();
  IGESControl_Controller::Init();

  // ---------------------------------------------------------------- ansamblul (mm)
  Handle(TDocStd_Document)  doc = make_doc(UnitsMethods_LengthUnit_Millimeter);
  Handle(XCAFDoc_ShapeTool) st  = XCAFDoc_DocumentTool::ShapeTool(doc->Main());
  Handle(XCAFDoc_ColorTool) ct  = XCAFDoc_DocumentTool::ColorTool(doc->Main());

  TopoDS_Shape box  = BRepPrimAPI_MakeBox(200.0, 100.0, 10.0).Shape();
  TDF_Label    boxL = st->AddShape(box, false);
  TDataStd_Name::Set(boxL, "Base plate");
  ct->SetColor(boxL, rgb(0.8, 0.1, 0.1), XCAFDoc_ColorSurf);
  // fata de sus (z = 10) galbena
  for (TopExp_Explorer e(box, TopAbs_FACE); e.More(); e.Next())
  {
    Bnd_Box b;
    BRepBndLib::Add(e.Current(), b);
    double x0, y0, z0, x1, y1, z1;
    b.Get(x0, y0, z0, x1, y1, z1);
    if (z0 > 9.9 && z1 < 10.1)
    {
      TDF_Label f = st->AddSubShape(boxL, e.Current());
      ct->SetColor(f, rgb(1.0, 0.85, 0.0), XCAFDoc_ColorSurf);
    }
  }

  TDF_Label pinL = st->AddShape(BRepPrimAPI_MakeCylinder(10.0, 50.0).Shape(), false);
  TDataStd_Name::Set(pinL, "Pin");
  ct->SetColor(pinL, rgb(0.1, 0.3, 0.9), XCAFDoc_ColorSurf);

  TDF_Label coneL = st->AddShape(BRepPrimAPI_MakeCone(20.0, 5.0, 30.0).Shape(), false);
  TDataStd_Name::Set(coneL, "Cone");
  ct->SetColor(coneL, rgb(0.1, 0.7, 0.2), XCAFDoc_ColorSurf);

  TDF_Label asmL = st->NewShape();
  TDataStd_Name::Set(asmL, "Bracket assembly");
  TDF_Label subL = st->NewShape();
  TDataStd_Name::Set(subL, "Cone sub-assembly");
  TDF_Label c;
  c = st->AddComponent(asmL, boxL, TopLoc_Location());
  TDataStd_Name::Set(c, "Base plate");
  c = st->AddComponent(asmL, pinL, at(50.0, 50.0, 10.0));
  TDataStd_Name::Set(c, "Pin:1");
  c = st->AddComponent(asmL, pinL, at(150.0, 50.0, 10.0));
  TDataStd_Name::Set(c, "Pin:2");
  c = st->AddComponent(subL, coneL, at(100.0, 50.0, 10.0));
  TDataStd_Name::Set(c, "Cone:1");
  c = st->AddComponent(asmL, subL, TopLoc_Location());
  TDataStd_Name::Set(c, "Cone sub-assembly");
  st->UpdateAssemblies();

  {
    Interface_Static::SetCVal("write.step.unit", "MM");
    STEPCAFControl_Writer w;
    w.SetColorMode(true);
    w.SetNameMode(true);
    if (!w.Transfer(doc, STEPControl_AsIs) || w.Write((dir + "/asm.step").c_str()) != IFSelect_RetDone)
    {
      printf("STEP write failed\n");
      return 1;
    }
  }
  {
    IGESCAFControl_Writer w;
    w.SetColorMode(true);
    w.SetNameMode(true);
    if (!w.Transfer(doc) || !w.Write((dir + "/asm.igs").c_str()))
    {
      printf("IGES write failed\n");
      return 1;
    }
  }

  // ---------------------------------------------------------------- cutie in metri
  {
    Handle(TDocStd_Document)  dm  = make_doc(UnitsMethods_LengthUnit_Meter);
    Handle(XCAFDoc_ShapeTool) stm = XCAFDoc_DocumentTool::ShapeTool(dm->Main());
    TDF_Label                 l   = stm->AddShape(BRepPrimAPI_MakeBox(2.0, 1.0, 0.5).Shape(), false);
    TDataStd_Name::Set(l, "Metre box");
    Interface_Static::SetCVal("write.step.unit", "M");
    STEPCAFControl_Writer w;
    if (!w.Transfer(dm, STEPControl_AsIs) || w.Write((dir + "/asm_m.step").c_str()) != IFSelect_RetDone)
    {
      printf("STEP (m) write failed\n");
      return 1;
    }
  }

  // ---------------------------------------------------------------- BREP
  {
    TopoDS_Compound comp;
    BRep_Builder    b;
    b.MakeCompound(comp);
    b.Add(comp, BRepPrimAPI_MakeBox(100.0, 50.0, 20.0).Shape());
    b.Add(comp, BRepPrimAPI_MakeSphere(gp_Pnt(300.0, 0.0, 0.0), 40.0).Shape());
    if (!BRepTools::Write(comp, (dir + "/parts.brep").c_str()))
    {
      printf("BREP write failed\n");
      return 1;
    }
  }
  printf("fixtures written to %s\n", dir.c_str());
  return 0;
}
