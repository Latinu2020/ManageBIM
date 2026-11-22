// Pregatire review arhitectura: selecteaza instalatiile (MEP) ca sa le poti
// ascunde/izola din meniul View (Hide Unselected / Isolate).
let mep = ["IfcPipeSegment", "IfcDuctSegment", "IfcCableCarrierSegment",
           "IfcFlowTerminal", "IfcFlowController", "Pipes", "Ducts"];

let mut total = 0;
for k in mep {
    total = total + select_where("Category", k);
}
log("Instalatii selectate: " + total.to_string());
zoom_to_selection();
log("Apasa I (Hide Unselected) sau Shift+I (Isolate) in View.");
