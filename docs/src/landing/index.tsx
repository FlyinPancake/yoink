import { Link } from "@tanstack/react-router";
import { HomeLayout } from "fumadocs-ui/layouts/home";
import { baseOptions } from "@/lib/layout.shared";

function Divider() {
  return <div className="border-t border-fd-border" />;
}

export function Landing() {
  return (
    <HomeLayout {...baseOptions()}>
      {/* Hero — split layout */}
      <section className="mx-auto max-w-6xl px-6 pt-16 pb-20 md:pt-28 md:pb-28">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-10 md:gap-16 items-center">
          <div className="animate-fade-in-up">
            <div className="flex items-center gap-4 mb-6">
              <img src="/yoink.svg" alt="" className="size-12 rounded-lg" />
              <h2 className="text-2xl font-bold">yoink</h2>
            </div>
            <h1 className="font-[Playfair_Display] text-5xl md:text-6xl lg:text-7xl font-bold leading-[1.1] mb-6">
              Your music
              <br />
              library,
              <br />
              <span className="italic text-blue-500">curated.</span>
            </h1>
            <p className="text-fd-muted-foreground leading-relaxed mb-8 max-w-md">
              yoink is a self-hosted music library manager that lets you grow
              your music collection from multiple sources, all from a single,
              clean web interface.
            </p>
            <div className="flex gap-3">
              <Link
                to="/docs/$"
                params={{ _splat: "" }}
                className="px-5 py-2.5 rounded-lg bg-blue-500 text-white font-medium text-sm hover:bg-blue-600 transition-colors"
              >
                Read the Docs
              </Link>
              <a
                href="https://github.com/FlyinPancake/yoink"
                target="_blank"
                rel="noopener noreferrer"
                className="px-5 py-2.5 rounded-lg border border-fd-border font-medium text-sm hover:bg-fd-accent transition-colors"
              >
                GitHub
              </a>
            </div>
          </div>
          <div className="animate-fade-in-up [animation-delay:200ms]">
            <img
              src="/screenshot-1.png"
              alt="yoink artist page"
              className="rounded-xl border border-fd-border shadow-xl w-full"
            />
          </div>
        </div>
      </section>

      <div className="mx-auto max-w-6xl px-6">
        <Divider />
      </div>

      {/* Section — Search */}
      <section className="mx-auto max-w-6xl px-6 py-20 md:py-28">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-10 md:gap-16 items-center">
          <img
            src="/screenshot-3.png"
            alt="Unified search"
            className="rounded-xl border border-fd-border shadow-lg order-last md:order-first"
          />
          <div>
            <p className="text-blue-500 font-medium text-sm mb-3 tracking-wide uppercase">
              Search
            </p>
            <h2 className="font-[Playfair_Display] text-3xl md:text-4xl font-bold mb-4 leading-tight">
              All your providers,
              <br />
              one query.
            </h2>
            <p className="text-fd-muted-foreground leading-relaxed">
              Search artists, albums, and tracks across many providers.
            </p>
          </div>
        </div>
      </section>

      <div className="mx-auto max-w-6xl px-6">
        <Divider />
      </div>

      {/* Section — Albums */}
      <section className="mx-auto max-w-6xl px-6 py-20 md:py-28">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-10 md:gap-16 items-center">
          <div>
            <p className="text-blue-500 font-medium text-sm mb-3 tracking-wide uppercase">
              Get
            </p>
            <h2 className="font-[Playfair_Display] text-3xl md:text-4xl font-bold mb-4 leading-tight">
              Download how you want it,
              <br />
              no compromises.
            </h2>
            <p className="text-fd-muted-foreground leading-relaxed">
              View full tracklists, match albums across providers, and download
              in whatever quality you need. From hi-res to mp3 quality, yoink
              has you covered.
            </p>
          </div>
          <img
            src="/screenshot-2.png"
            alt="Album detail"
            className="rounded-xl border border-fd-border shadow-lg"
          />
        </div>
      </section>

      <div className="mx-auto max-w-6xl px-6">
        <Divider />
      </div>

      {/* Section — Artist monitoring */}
      {/*<section className="mx-auto max-w-6xl px-6 py-20 md:py-28">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-10 md:gap-16 items-center">
          <img
            src="/screenshot-4.png"
            alt="Artist management"
            className="rounded-xl border border-fd-border shadow-lg order-last md:order-first"
          />
          <div>
            <p className="text-blue-500 font-medium text-sm mb-3 tracking-wide uppercase">
              Automate
            </p>
            <h2 className="font-[Playfair_Display] text-3xl md:text-4xl font-bold mb-4 leading-tight">
              Track what
              <br />
              matters to you.
            </h2>
            <p className="text-fd-muted-foreground leading-relaxed">
              Add artists as lightweight entries or promote them to fully
              monitored with automatic discography sync. Combine metadata from
              all your providers into unified, rich artist pages.
            </p>
          </div>
        </div>
      </section>*/}

      <div className="mx-auto max-w-6xl px-6">
        <Divider />
      </div>

      {/* Tech & CTA */}
      <section className="mx-auto max-w-3xl px-6 py-20 md:py-28 text-center">
        <p className="text-blue-500 font-medium text-sm mb-3 tracking-wide uppercase">
          Get Started
        </p>
        <h2 className="font-[Playfair_Display] text-3xl md:text-4xl font-bold mb-4">
          Up and running in minutes.
        </h2>
        <div className="flex items-center justify-center gap-3">
          <Link
            to="/docs/$"
            params={{ _splat: "getting-started" }}
            className="px-5 py-2.5 rounded-lg bg-blue-500 text-white font-medium text-sm hover:bg-blue-600 transition-colors"
          >
            Get Started
          </Link>
          <a
            href="https://github.com/FlyinPancake/yoink"
            target="_blank"
            rel="noopener noreferrer"
            className="px-5 py-2.5 rounded-lg border border-fd-border font-medium text-sm hover:bg-fd-accent transition-colors"
          >
            View Source
          </a>
        </div>
      </section>
    </HomeLayout>
  );
}
