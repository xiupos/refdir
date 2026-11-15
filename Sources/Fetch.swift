// Papers as BibTeX from INSPIRE-HEP, or from doi.org for a DOI it does not have, and their PDFs
// from arXiv. What it asks and how it reads the answers is Core's; this is only the network

import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// A short sentence, shown as it is
struct Failure: LocalizedError {
    let errorDescription: String?

    init(_ message: String) {
        errorDescription = message
    }
}

enum Fetch {
    /// The program and where to find it, as INSPIRE asks of its clients
    private static let userAgent: String = {
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0"
        return "Refdir/\(version) (iOS; https://github.com/xiupos/RefdirApp)"
    }()

    /// Without a cache: INSPIRE's and doi.org's, 30 seconds; arXiv's, 5 minutes
    private static let short = session(30)
    private static let long = session(300)

    private static func session(_ seconds: Double) -> URLSession {
        let config = URLSessionConfiguration.ephemeral
        config.urlCache = nil
        config.requestCachePolicy = .reloadIgnoringLocalCacheData
        config.timeoutIntervalForResource = seconds
        config.httpAdditionalHeaders = ["User-Agent": userAgent]
        return URLSession(configuration: config)
    }

    /// The one paper an identifier finds: INSPIRE's BibTeX, or else, for a DOI, doi.org's
    static func find(_ id: String) async throws -> Bib {
        let q = inspireQuery(id)
        let url = URL(string: "https://inspirehep.net/api/literature?q=\(percentEncoded(q))&size=2&format=bibtex")!
        let found = try parse(await get(url))
        switch found.count {
        case 1:
            return found[0]
        case 0:
            guard let doi = doiOf(query: q) else { throw Failure("INSPIRE finds nothing for \(id.trimmed).") }
            return try await fromDOI(doi)
        default:
            throw Failure("INSPIRE finds more than one paper for \(id.trimmed).")
        }
    }

    /// doi.org's BibTeX for a DOI, with its month as BibTeX's macro
    private static func fromDOI(_ doi: String) async throws -> Bib {
        let path = doi.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? doi
        guard let url = URL(string: "https://doi.org/\(path)") else { throw Failure("\(doi) is not a DOI.") }
        guard let bib = try parse(await get(url, accept: "application/x-bibtex")).first else {
            throw Failure("doi.org has no BibTeX for \(doi).")
        }
        return normalizeMonths(bib)
    }

    /// The entry's paper again from INSPIRE, by its arXiv number or DOI, with its key, tags, and note
    static func again(_ entry: Entry) async throws -> Bib {
        guard let q = againQuery(entry.bib) else { throw Failure("It has no arXiv number or DOI.") }
        return keepingOwn(try await find(q), of: entry)
    }

    /// Downloads the entry's PDF from arXiv into its folder, as `<arXiv number>.pdf`
    static func pdf(_ entry: Entry) async throws -> URL {
        guard let arxiv = entry.bib.get("eprint"),
              let url = URL(string: "https://arxiv.org/pdf/\(arxiv)") else { throw Failure("It has no arXiv number.") }
        let data = try await get(url, session: long)
        guard data.starts(with: Data("%PDF-".utf8)) else { throw Failure("arXiv gave no PDF.") }
        return try entry.addFile(arxivPDFName(arxiv), data)
    }

    private static func parse(_ data: Data) throws -> [Bib] {
        try Bib.parse(String(decoding: data, as: UTF8.self))
    }

    /// A response. Errors are short sentences naming the host; the details go to the console
    private static func get(_ url: URL, accept: String? = nil, session: URLSession = short) async throws -> Data {
        var request = URLRequest(url: url)
        if let accept { request.setValue(accept, forHTTPHeaderField: "Accept") }
        let host = url.host ?? url.absoluteString
        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: request)
        } catch {
            print("\(url): \(error)")
            throw Failure("\(host) cannot be reached.")
        }
        switch (response as? HTTPURLResponse)?.statusCode ?? 0 {
        case 200..<300: return data
        case 404: throw Failure("\(host) does not have it.")
        case let code: throw Failure("\(host) answered with error \(code).")
        }
    }
}
