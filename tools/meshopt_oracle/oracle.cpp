// The reference values for viewer-shell/meshopt-port's tests: meshoptimizer 1.3, built
// from its own source, run over the parts `cargo run --example dump_parts` writes. See
// README.md here. Prints one tab-separated line per part (oracle_values.tsv's format).
//
// Per part, for the order as the file has it (A) and after meshoptimizer's vertex cache
// pass (B; the same as Gardenfell's triorder, byte for byte):
//   the fetch remap's kept count and hash, the remapped index list's hash, the interleaved
//   fetch's output hash (positions, 12 bytes a vertex), the vertex fetch statistics at 32
//   bytes a vertex before and after the remap, the overdraw statistics; and for A the
//   coverage. Floats are printed as their bit patterns, so equal means equal.
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

static void order(const std::vector<unsigned int>& idx, const std::vector<float>& pos, size_t nv)
{
	std::vector<unsigned int> remap(nv);
	size_t kept = meshopt_optimizeVertexFetchRemap(remap.data(), idx.data(), idx.size(), nv);
	std::vector<unsigned int> ridx(idx.size());
	meshopt_remapIndexBuffer(ridx.data(), idx.data(), idx.size(), remap.data());

	std::vector<unsigned int> fidx = idx;
	std::vector<float> fpos(nv * 3);
	size_t fkept = meshopt_optimizeVertexFetch(fpos.data(), fidx.data(), fidx.size(), pos.data(), nv, 12);
	std::vector<unsigned int> fwords(fkept * 3);
	memcpy(fwords.data(), fpos.data(), fkept * 12);

	meshopt_VertexFetchStatistics vb = meshopt_analyzeVertexFetch(idx.data(), idx.size(), nv, 32);
	meshopt_VertexFetchStatistics va = meshopt_analyzeVertexFetch(ridx.data(), ridx.size(), kept, 32);
	meshopt_OverdrawStatistics od = meshopt_analyzeOverdraw(idx.data(), idx.size(), pos.data(), nv, 12);

	printf("\t%zu\t%016llx\t%016llx\t%016llx\t%u\t%08x\t%u\t%08x\t%u\t%u\t%08x", kept, (unsigned long long)fnv(remap.data(), nv),
	    (unsigned long long)fnv(ridx.data(), ridx.size()), (unsigned long long)fnv(fwords.data(), fwords.size()), vb.bytes_fetched, bits(vb.overfetch),
	    va.bytes_fetched, bits(va.overfetch), od.pixels_covered, od.pixels_shaded, bits(od.overdraw));
}

int main(int argc, char** argv)
{
	if (argc < 2)
	{
		fprintf(stderr, "usage: oracle <parts.bin>\n");
		return 2;
	}
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

		printf("%s\t%zu\t%zu", name.c_str(), nv, ni);
		order(idx, pos, nv);
		meshopt_CoverageStatistics cv = meshopt_analyzeCoverage(idx.data(), idx.size(), pos.data(), nv, 12);
		printf("\t%08x\t%08x\t%08x\t%08x", bits(cv.coverage[0]), bits(cv.coverage[1]), bits(cv.coverage[2]), bits(cv.extent));

		std::vector<unsigned int> b(ni);
		meshopt_optimizeVertexCache(b.data(), idx.data(), ni, nv);
		printf("\t%016llx", (unsigned long long)fnv(b.data(), ni));
		order(b, pos, nv);
		printf("\n");
	}
	fclose(f);
	return 0;
}
