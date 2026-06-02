# tor-dirpublish

Code to publish Tor directory information.

## Overview

This crate is part of
[Arti](https://gitlab.torproject.org/tpo/core/arti/), a project to
implement [Tor](https://www.torproject.org/) in Rust.

This crate's functionality is to ensure that a document is published
to one or more "targets"—typically, Tor directory servers.
If a publication attempt to a target fails,
it is retried until it succeeds.

This functionality was originally written for relays to publish their
router descriptors and extrainfo documents to directory authorities.
It should also serve
for the parts of the directory consensus protocol
where authorities upload votes and signatures to one another.
Eventually, it might be used to replace the current
hidden service descriptor publication code in `tor-hsservice`.

## Changes and the publication process

The document, and the list of targets, are both allowed to change.
If the document changes, then the new version is published to all targets.
If the targets change,
then any existing document is published to all new targets,
and any pending retries to old targets are forgotten.
If an upload attempt is in-flight when the document changes
or the a target is removed, it is allowed to run to completion.

## Using this crate

To publish a document, construct an [`Uploader`] that handles that
document.  If you're uploading to IP addresses over over unencrypted HTTP,
and you're using `tor-dirclient` as your request engine,
[`http::DirectHttpUploader`] should meet your needs.
Otherwise, you'll need to define your own uploader implementation.

Each uploader has its types for documents and targets.
for `DirectHttpUploader`, the documents are `tor_dirclient::Requestable`s,
and the targets are lists of IP addresses.

Pass this uploader, along with your initial document and list of targets,
to [`Publisher::launch`], which will create a new background task
that tries to use that uploader to keep the latest version of the document
published to every backend.

You can change the current document with [`Publisher::set_document`]
and adjust the targets of publication with [`Publisher::adjust_targets`].
To temporarily pause publication, set the document to None.


----

License: MIT OR Apache-2.0
