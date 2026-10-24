using OfficeIMO.OneNote;
using OfficeIMO.OneNote.Html;
using OfficeIMO.OneNote.Markdown;

static string Required(string[] args, string name)
{
    var index = Array.IndexOf(args, name);
    if (index < 0 || index + 1 >= args.Length || string.IsNullOrWhiteSpace(args[index + 1]))
        throw new ArgumentException($"Missing required argument {name}.");
    return args[index + 1];
}

try
{
    var source = Path.GetFullPath(Required(args, "--source"));
    var output = Path.GetFullPath(Required(args, "--out"));
    var format = Required(args, "--format").ToLowerInvariant();
    if (!File.Exists(source) || !source.EndsWith(".one", StringComparison.OrdinalIgnoreCase))
        throw new ArgumentException("--source must name an existing .one section file.");
    if (format is not ("html" or "cmgfm"))
        throw new ArgumentException("--format must be html or cmgfm.");

    Directory.CreateDirectory(output);
    var section = OneNoteSectionReader.Read(source);
    if (format == "html")
        File.WriteAllText(Path.Combine(output, "index.html"), section.ToHtmlDocument());
    else
        File.WriteAllText(Path.Combine(output, "index.md"), section.ToMarkdownDocument(new OneNoteMarkdownOptions()).ToMarkdown());
    Console.WriteLine($"Imported {Path.GetFileName(source)} to {output}");
    return 0;
}
catch (Exception error)
{
    Console.Error.WriteLine(error.Message);
    return 1;
}
