// The simplifier's reference values for viewer-shell/meshopt-port (tests/simplify.rs):
// meshoptimizer 1.3's simplifier, built from its own source, over the parts
// `cargo run --example dump_parts` writes. One tab-separated line per part.
//
// Per part (attributes, locks and colours made the same way in tests/simplify.rs, from
// positions and vertex numbers only, plain float arithmetic):
//   simplify at a quarter of the triangles, error 1e-2, with four option sets;
//   simplifyWithAttributes (2 attributes, locks/protect/priority) with two option sets;
//   simplifyWithUpdate (the same), with the moved positions and attributes hashed;
//   simplifySloppy, simplifyPrune, simplifyPoints and simplifyScale.
// For each: the count, the result's hash, and the error's bit pattern where there is one.
#include "meshoptimizer.h"

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

static uint64_t fnv(const unsigned int* v, size_t n)
{
	uint64_t h = 1469598103934665603ull;
	for (size_t i = 0; i < n; ++i)
		for (int k = 0; k < 4; ++k)
		{
			h ^= (v[i] >> (k * 8)) & 255;
			h *= 1099511628211ull;
		}
	return h;
}

static unsigned int bits(float f)
{
	unsigned int u;
	memcpy(&u, &f, 4);
	return u;
}

static void out(size_t n, const unsigned int* v, float err)
{
	printf("\t%zu\t%016llx\t%08x", n, (unsigned long long)fnv(v, n), bits(err));
}

int main(int argc, char** argv)
{
	if (argc < 2)
		return 2;
	FILE* f = fopen(argv[1], "rb");
	if (!f)
		return 1;
	auto u32 = [&]() { unsigned int v = 0; if (fread(&v, 4, 1, f) != 1) v = 0; return v; };
	unsigned int count = u32();
	for (unsigned int p = 0; p < count; ++p)
	{
		std::string name(u32(), '\0');
		if (!name.empty() && fread(&name[0], 1, name.size(), f) != name.size())
			return 1;
		size_t nv = u32(), ni = u32();
		std::vector<float> pos(nv * 3);
		std::vector<unsigned int> idx(ni);
		if (fread(pos.data(), 4, pos.size(), f) != pos.size() || fread(idx.data(), 4, idx.size(), f) != idx.size())
			return 1;

		// two attributes per vertex, discontinuous where positions are shared
		std::vector<float> attr(nv * 2);
		std::vector<unsigned char> lock(nv);
		for (size_t i = 0; i < nv; ++i)
		{
			attr[i * 2 + 0] = pos[i * 3 + 0] * 0.5f + pos[i * 3 + 1];
			attr[i * 2 + 1] = float(i % 5) * 0.25f;
			lock[i] = (i % 7 == 0 ? 1 : 0) | (i % 11 == 0 ? 4 : 0) | (i % 13 == 0 ? 2 : 0);
		}
		const float weights[2] = {1.f, 0.5f};

		size_t target = (ni / 3 / 4) * 3;
		std::vector<unsigned int> dst(ni);
		float err = 0;

		printf("%s\t%zu\t%zu", name.c_str(), nv, ni);

		const unsigned int opts[4] = {0, meshopt_SimplifyLockBorder | meshopt_SimplifyPrune, meshopt_SimplifyRegularize | meshopt_SimplifyPreserveFolds | meshopt_SimplifyErrorAbsolute, meshopt_SimplifyRegularizeLight | meshopt_SimplifySparse};
		for (unsigned int o : opts)
		{
			size_t n = meshopt_simplify(dst.data(), idx.data(), ni, pos.data(), nv, 12, target, o & meshopt_SimplifyErrorAbsolute ? 1e-1f : 1e-2f, o, &err);
			out(n, dst.data(), err);
		}

		const unsigned int aopts[2] = {0, meshopt_SimplifyPermissive | meshopt_SimplifyErrorClamped};
		for (unsigned int o : aopts)
		{
			size_t n = meshopt_simplifyWithAttributes(dst.data(), idx.data(), ni, pos.data(), nv, 12, attr.data(), 8, weights, 2, lock.data(), target, 1e-2f, o, &err);
			out(n, dst.data(), err);
		}

		{
			std::vector<unsigned int> uidx = idx;
			std::vector<float> upos = pos, uattr = attr;
			size_t n = meshopt_simplifyWithUpdate(uidx.data(), ni, upos.data(), nv, 12, uattr.data(), 8, weights, 2, lock.data(), target, 1e-2f, 0, &err);
			out(n, uidx.data(), err);
			std::vector<unsigned int> pw(upos.size()), aw(uattr.size());
			memcpy(pw.data(), upos.data(), upos.size() * 4);
			memcpy(aw.data(), uattr.data(), uattr.size() * 4);
			printf("\t%016llx\t%016llx", (unsigned long long)fnv(pw.data(), pw.size()), (unsigned long long)fnv(aw.data(), aw.size()));
		}

		{
			size_t n = meshopt_simplifySloppy(dst.data(), idx.data(), ni, pos.data(), nv, 12, NULL, target, 2e-2f, &err);
			out(n, dst.data(), err);
			n = meshopt_simplifySloppy(dst.data(), idx.data(), ni, pos.data(), nv, 12, lock.data(), target, 0.f, &err);
			out(n, dst.data(), err);
		}

		{
			size_t n = meshopt_simplifyPrune(dst.data(), idx.data(), ni, pos.data(), nv, 12, 5e-2f);
			out(n, dst.data(), 0.f);
		}

		{
			std::vector<unsigned int> pts(nv);
			// colours: the positions themselves (the attributes are two floats, colours three)
			size_t n = meshopt_simplifyPoints(pts.data(), pos.data(), nv, 12, pos.data(), 12, 1.f, nv / 4);
			out(n, pts.data(), 0.f);
		}

		printf("\t%08x\n", bits(meshopt_simplifyScale(pos.data(), nv, 12)));
	}
	fclose(f);
	return 0;
}
