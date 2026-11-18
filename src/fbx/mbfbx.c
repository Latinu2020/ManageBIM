// mbfbx - citire FBX pentru ManageBIM, peste ufbx (MIT, third_party/ufbx). Legat static (licenta MIT).
// O "piesa" = un nod cu mesh: triunghiuri in coordonate WORLD (metri, Z in sus) + culoare per triunghi.
#include <stdlib.h>
#include <string.h>
#include "../../third_party/ufbx/ufbx.h"

typedef struct {
    char *name, *path;
    double *xyz;          // 9 / triunghi
    unsigned char *rgba;  // 4 / triunghi
    int ntri;
} MbPart;

typedef struct {
    MbPart *parts;
    int count;
    char error[512];
    double unit;          // metri per unitate in fisier
    char app[128];
} MbFbx;

static char *dup_str(const char *s, size_t n) {
    char *d = (char *)malloc(n + 1);
    if (d) { memcpy(d, s, n); d[n] = 0; }
    return d;
}

static unsigned char to_u8(double v) {
    if (v < 0) v = 0;
    if (v > 1) v = 1;
    return (unsigned char)(v * 255.0 + 0.5);
}

static void material_rgba(const ufbx_material *m, unsigned char out[4]) {
    out[0] = out[1] = out[2] = 190; out[3] = 255;
    if (!m) return;
    ufbx_vec4 c;
    double opacity = 1.0;
    if (m->pbr.base_color.has_value) c = m->pbr.base_color.value_vec4;
    else if (m->fbx.diffuse_color.has_value) c = m->fbx.diffuse_color.value_vec4;
    else return;
    if (m->pbr.opacity.has_value) opacity = m->pbr.opacity.value_real;
    else if (m->fbx.transparency_factor.has_value) opacity = 1.0 - m->fbx.transparency_factor.value_real;
    out[0] = to_u8(c.x); out[1] = to_u8(c.y); out[2] = to_u8(c.z); out[3] = to_u8(opacity > 0.05 ? opacity : 1.0);
}

MbFbx *mbfbx_load(const char *path) {
    MbFbx *r = (MbFbx *)calloc(1, sizeof(MbFbx));
    if (!r) return NULL;
    ufbx_load_opts opts;
    memset(&opts, 0, sizeof(opts));
    opts.target_axes = ufbx_axes_right_handed_z_up;
    opts.target_unit_meters = 1.0;
    opts.space_conversion = UFBX_SPACE_CONVERSION_MODIFY_GEOMETRY;
    ufbx_error err;
    ufbx_scene *scene = ufbx_load_file(path, &opts, &err);
    if (!scene) {
        ufbx_format_error(r->error, sizeof(r->error), &err);
        return r;
    }
    r->unit = scene->settings.unit_meters;
    if (scene->metadata.creator.length) {
        size_t n = scene->metadata.creator.length < 127 ? scene->metadata.creator.length : 127;
        memcpy(r->app, scene->metadata.creator.data, n);
    }
    r->parts = (MbPart *)calloc(scene->nodes.count ? scene->nodes.count : 1, sizeof(MbPart));
    for (size_t ni = 0; ni < scene->nodes.count; ni++) {
        ufbx_node *node = scene->nodes.data[ni];
        ufbx_mesh *mesh = node->mesh;
        if (!mesh || !node->visible || mesh->num_triangles == 0) continue;
        MbPart *p = &r->parts[r->count];
        p->xyz = (double *)malloc(mesh->num_triangles * 9 * sizeof(double));
        p->rgba = (unsigned char *)malloc(mesh->num_triangles * 4);
        uint32_t *idx = (uint32_t *)malloc((mesh->max_face_triangles ? mesh->max_face_triangles : 1) * 3 * sizeof(uint32_t));
        if (!p->xyz || !p->rgba || !idx) { free(idx); continue; }
        for (size_t fi = 0; fi < mesh->faces.count; fi++) {
            ufbx_face face = mesh->faces.data[fi];
            if (mesh->face_hole.count > fi && mesh->face_hole.data[fi]) continue;
            uint32_t nt = ufbx_triangulate_face(idx, mesh->max_face_triangles * 3, mesh, face);
            uint32_t mi = mesh->face_material.count > fi ? mesh->face_material.data[fi] : 0;
            const ufbx_material *mat = node->materials.count > mi ? node->materials.data[mi] : (mesh->materials.count > mi ? mesh->materials.data[mi] : NULL);
            unsigned char c[4];
            material_rgba(mat, c);
            for (uint32_t t = 0; t < nt && (size_t)p->ntri < mesh->num_triangles; t++) {
                for (int k = 0; k < 3; k++) {
                    uint32_t vi = mesh->vertex_position.indices.data[idx[t * 3 + k]];
                    ufbx_vec3 v = ufbx_transform_position(&node->geometry_to_world, mesh->vertex_position.values.data[vi]);
                    p->xyz[p->ntri * 9 + k * 3 + 0] = v.x;
                    p->xyz[p->ntri * 9 + k * 3 + 1] = v.y;
                    p->xyz[p->ntri * 9 + k * 3 + 2] = v.z;
                }
                memcpy(p->rgba + p->ntri * 4, c, 4);
                p->ntri++;
            }
        }
        free(idx);
        if (p->ntri == 0) { free(p->xyz); free(p->rgba); memset(p, 0, sizeof(*p)); continue; }
        p->name = dup_str(node->name.data, node->name.length);
        // calea in ierarhie: parintii (fara radacina)
        char buf[1024]; size_t len = 0; buf[0] = 0;
        const ufbx_node *chain[64]; int depth = 0;
        for (const ufbx_node *q = node->parent; q && !q->is_root && depth < 64; q = q->parent) chain[depth++] = q;
        for (int d = depth - 1; d >= 0; d--) {
            const ufbx_string *s = &chain[d]->name;
            if (len + s->length + 4 >= sizeof(buf)) break;
            if (len) { memcpy(buf + len, " / ", 3); len += 3; }
            memcpy(buf + len, s->data, s->length); len += s->length;
        }
        p->path = dup_str(buf, len);
        r->count++;
    }
    ufbx_free_scene(scene);
    if (r->count == 0) strcpy(r->error, "No 3D meshes in this FBX file.");
    return r;
}

const char *mbfbx_error(const MbFbx *r) { return r ? r->error : "null"; }
double mbfbx_unit(const MbFbx *r) { return r ? r->unit : 0; }
const char *mbfbx_app(const MbFbx *r) { return r ? r->app : ""; }
int mbfbx_count(const MbFbx *r) { return r ? r->count : 0; }
const char *mbfbx_name(const MbFbx *r, int i) { return r->parts[i].name ? r->parts[i].name : ""; }
const char *mbfbx_path(const MbFbx *r, int i) { return r->parts[i].path ? r->parts[i].path : ""; }
int mbfbx_ntri(const MbFbx *r, int i) { return r->parts[i].ntri; }
const double *mbfbx_xyz(const MbFbx *r, int i) { return r->parts[i].xyz; }
const unsigned char *mbfbx_rgba(const MbFbx *r, int i) { return r->parts[i].rgba; }

void mbfbx_free(MbFbx *r) {
    if (!r) return;
    for (int i = 0; i < r->count; i++) {
        free(r->parts[i].name); free(r->parts[i].path); free(r->parts[i].xyz); free(r->parts[i].rgba);
    }
    free(r->parts);
    free(r);
}
