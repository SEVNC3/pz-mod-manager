# Workshop parity queue

Goal: PZMM's Workshop tab covers everything Steam's Project Zomboid Workshop
offers a logged-out visitor. The list comes from an audit on 2026-10-10. Every
URL parameter below was fetched and checked anonymously.

Data sources:
- **Browse page.** `steamcommunity.com/workshop/browse/?appid=108600` returns its results as JSON in `<script id="valve-ssr-data">`. Each result carries `views`, `favorited`, `lifetime_subscriptions`, `num_comments_public`, `time_created`, `star_rating`, `total_votes`, `file_size`, `tags`, `creator` and the creator's name.
- **Keyless API.** `ISteamRemoteStorage/GetPublishedFileDetails` returns the full BBCode description and counts. `GetCollectionDetails` returns collection members.
- **Not usable.** `IPublishedFileService/QueryFiles` needs a key.

Code: `browse()` and its helpers in `crates/core/src/sources/steam_workshop.rs`, the `workshop_browse` command in `app/src-tauri/src/lib.rs`, `app/src/lib/api.ts`, `app/src/lib/Workshop.svelte` (sidebar, Home, results) and `app/src/lib/WorkshopPanel.svelte` (the item panel).

## P1

1. [x] **Tags: include and exclude.** A chip bar of PZ's tags, where clicking cycles off, include, exclude. Tags are AND-ed; Steam has no "match any".
   - `browse()` takes `required` and `excluded` lists, sent as `requiredtags[]` and `excludedtags[]`.
   - Tag counts come from the hub's `workshop_tag_facets`, if they're cheap to fetch.
2. [x] **Time range for Popular.** `days=1|7|30|90|180|365` (Today, Week, Month, 3 months, 6 months, Year), shown when the sort is Popular. Replaces the hard-coded 7.
3. [x] **Richer cards.** Show views, favourites, comment count, created date, file size and author from data already in the results. `Item::from_json` keeps them.
4. [x] **Item detail panel.** Opened from any card. Contains:
   - the screenshot gallery, scraped from `rgFullScreenshotURLs` on the item page
   - the full description (BBCode rendered safely) and tags
   - counts, created and updated dates, file size and author
   - required items (`parse_required`)
   - Install / Installed and Workshop page buttons
5. [x] **Dependents and libraries.**
   - "Used by N items" lists the items that depend on this one (`childpublishedfileid=ID`).
   - A "Most required" sort shows the libraries (`special_filter=6`, by subscribers).
6. [x] **Collections.**
   - An Items / Collections toggle (`section=collections`). Collections sort only by trend, top rated, newest and last updated.
   - A collection view lists its members (`GetCollectionDetails` plus `details()`) with Install all.
   - "In N collections" on the detail panel (`childpublishedfileid=ID&section=collections`).

## P2

7. [x] **Home view with rows,** like Steam's hub: Trending this week, Most subscribed, New, Last updated, Most required. Each row is a horizontal scroller of 12 items from `browse()`, with "View all" opening Browse with that sort.
8. [x] **Search scope and syntax.**
   - Scope: All / Title / Description (`search_text_target=0|1|2`).
   - A hint for Steam's `AND OR NOT + -` operators.
9. [x] **Page size** 30 or 50 (`num_per_page`; Steam ignores other values).
10. [x] **Date filters.** Created or updated between two dates: `created_date_range_filter_start`/`_end` and `updated_date_range_filter_start`/`_end`, as unix times.
11. [x] **Change notes tab** in the detail panel: `/sharedfiles/filedetails/changelog/ID?p=N`, parsed from HTML.
12. [x] **Comments tab,** read-only and paged: `POST /comment/PublishedFile_Public/render/{creator}/{id}/` with `start&count`, rendered as sanitised text.
13. [x] **Author's items.** Click an author to list their PZ items: `/profiles/{creator}/myworkshopfiles/?appid=108600&p=N`, scraped.
14. [x] **Sort by "most collections"** (`num_parent_collections`).

## P3

15. [x] **Incompatible items:** a filter and badge (`requiredflags[]=incompatible`).
16. [x] **Videos in the gallery:** YouTube links from the item page.

## Not doing

- **Login-only features.** Your Items, subscriptions, favourites, voting, posting comments, Subscribe, Add to collection, friends filters and saved searches all need a logged-in session. PZMM's own "Installed" state covers subscriptions.
- **Playtime sorts.** Project Zomboid doesn't track playtime, so these return the same order as Newest.
- **Deep paging.** Steam caps results at 1000 pages.
