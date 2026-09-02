//! The `keys` subcommand.
// TODO: The output of these subcommands needs improvement. Also, some of the `display_` functions
// are repetitive and redundant.

use std::str::FromStr;

use anyhow::Result;

use arti_client::{InertTorClient, TorClient, TorClientConfig};
use clap::{ArgMatches, Args, FromArgMatches, Parser, Subcommand};
use tor_keymgr::{
    KeyMgr, KeyPathInfo, KeystoreEntry, KeystoreEntryResult, KeystoreId, UnrecognizedEntryError,
};
use tor_rtcompat::Runtime;

use crate::{ArtiConfig, subcommands::prompt};

#[cfg(feature = "onion-service-service")]
use tor_hsservice::OnionService;

/// The `keys` subcommands the arti CLI will be augmented with.
#[derive(Debug, Parser)]
pub(crate) enum KeysSubcommands {
    /// Run keystore management commands.
    #[command(subcommand)]
    Keys(KeysSubcommand),
}

/// The `keys` subcommand.
#[derive(Subcommand, Debug, Clone)]
pub(crate) enum KeysSubcommand {
    /// List keys and certificates.
    ///
    /// Note: The output fields "Location" and "Keystore ID" represent,
    /// respectively, the raw identifier of an entry (e.g. <ARTI_PATH>.<ENTRY_TYPE>
    /// for `ArtiNativeKeystore`), and the identifier of the keystore that
    /// contains the entry.
    List(ListArgs),

    /// List keystores.
    ListKeystores,

    /// Validate the integrity of keystores.
    ///
    /// Detects and reports unrecognized entries and paths, as well as
    /// malformed or expired keys.
    ///
    /// Such entries will be removed if this command is invoked with `--sweep`.
    CheckIntegrity(CheckIntegrityArgs),
}

/// The arguments of the [`List`](KeysSubcommand::List) subcommand.
#[derive(Debug, Clone, Args)]
pub(crate) struct ListArgs {
    /// Identifier of the keystore.
    ///
    /// If omitted, keys and certificates
    /// from all the keystores will be returned.
    #[arg(short, long)]
    keystore_id: Option<String>,

    /// Output format.
    #[command(flatten)]
    output_format: OutputFormat,
}

/// Mutually exclusive output format flags.
// NOTE: Additional output formats will be added in the future.
#[derive(Debug, Clone, Args)]
#[group(multiple = false)]
struct OutputFormat {
    /// Compact format.
    ///
    /// Displays every entry on a single line when enabled.
    #[arg(long, default_value_t = false)]
    compact: bool,
}

/// The arguments of the [`CheckIntegrity`](KeysSubcommand::CheckIntegrity) subcommand.
#[derive(Debug, Clone, Args)]
pub(crate) struct CheckIntegrityArgs {
    /// Identifier of the keystore.
    ///
    /// If omitted, keys and certificates
    /// from all the keystores will be checked.
    #[arg(short, long)]
    keystore_id: Option<KeystoreId>,

    /// Remove the detected invalid keystore entries.
    #[arg(long, short, default_value_t = false)]
    sweep: bool,

    /// With this flag active no prompt will be shown
    /// and no confirmation will be asked.
    // TODO: Rephrase this and the `batch` flags of the
    // other commands in the present tense.
    #[arg(long, short, default_value_t = false)]
    batch: bool,
}

/// A set of invalid keystore entries associated with a keystore ID.
/// This struct is used solely to reduce type complexity; it does not
/// perform any validation (e.g., whether the entries actually belong
/// to the keystore indicated by the ID).
#[derive(Clone)]
struct InvalidKeystoreEntries<'a> {
    /// The `KeystoreId` that the entries are expected to belong to.
    keystore_id: KeystoreId,
    /// The list of invalid entries that logically belong to the keystore identified
    /// by `keystore_id`.
    entries: Vec<InvalidKeystoreEntry<'a>>,
}

/// An invalid keystore entry associated with the error that caused it to be
/// invalid. This struct is used solely to reduce type complexity; it does not
/// perform any validation (e.g., whether the `error_msg` actually corresponds
/// to the error that caused the invalid entry).
#[derive(Clone)]
struct InvalidKeystoreEntry<'a> {
    /// The entry
    entry: KeystoreEntryResult<KeystoreEntry<'a>>,
    /// The error message derived from the error that caused the entry to be invalid.
    /// This field is needed (even if `Err(UnrecognizedEntryError)` contains the error) because `Ok(KeystoreEntry)`s could be invalid too.
    error_msg: String,
}

/// Run the `keys` subcommand.
pub(crate) fn run<R: Runtime>(
    runtime: R,
    keys_matches: &ArgMatches,
    config: &ArtiConfig,
    client_config: &TorClientConfig,
) -> Result<()> {
    let subcommand =
        KeysSubcommand::from_arg_matches(keys_matches).expect("Could not parse keys subcommand");
    let rt = runtime.clone();
    let client_builder = TorClient::with_runtime(runtime).config(client_config.clone());

    match subcommand {
        KeysSubcommand::List(args) => run_list_keys(args, &client_builder.create_inert()?),
        KeysSubcommand::ListKeystores => run_list_keystores(&client_builder.create_inert()?),
        KeysSubcommand::CheckIntegrity(args) => run_check_integrity(
            &args,
            rt.reenter_block_on(client_builder.create_bootstrapped())?
                .as_ref(),
            config,
            client_config,
        ),
    }
}

/// Print information about a valid keystore entry.
fn display_entry(entry: &(KeystoreEntry<'_>, KeyPathInfo), display_keystore_id: bool) {
    let (entry, info) = entry;
    if display_keystore_id {
        println!("Keystore ID: {}", entry.keystore_id());
    }
    println!("Role: {}", info.role());
    println!("Summary: {}", info.summary());
    println!("KeystoreItemType: {:?}", entry.key_type());
    println!("Location: {}", entry.raw_id());
    let extra_info = info.extra_info();
    println!("Extra info:");
    for (key, value) in extra_info {
        println!("- {key}: {value}");
    }
}

/// Print information about an unrecognized keystore entry.
fn display_unrecognized_entry(
    entry: &UnrecognizedEntryError,
    display_keystore_id: bool,
    compact_output: bool,
) {
    let raw_entry = entry.entry();
    #[allow(clippy::single_match)]
    match raw_entry.raw_id() {
        tor_keymgr::RawEntryId::Path(p) => {
            let path = p.to_string_lossy();
            if compact_output {
                println!("{path}");
            } else {
                if display_keystore_id {
                    println!("Keystore ID: {}", raw_entry.keystore_id());
                }
                println!("Location: {path}");
                println!("Error: {}", entry.error());
                println!();
            }
        }
        // NOTE: For the time being Arti only supports
        // on-disk keystores, but more supported medium
        // will be added.
        other => {
            panic!("Unhandled enum variant: {:?}", other);
        }
    }
}

/// Run the `keys list` subcommand.
fn run_list_keys(args: ListArgs, client: &InertTorClient) -> Result<()> {
    let keymgr = client.keymgr()?;
    let (display_keystore_id, entries) = if let Some(id) = args.keystore_id {
        let id = KeystoreId::from_str(&id)?;
        let entries = keymgr.list_by_id(&id)?;
        if entries.is_empty() {
            return Ok(());
        }
        (false, entries)
    } else {
        let entries = keymgr.list()?;
        if entries.is_empty() {
            return Ok(());
        }
        (true, entries)
    };

    let (mut valid_entries, mut unrecognized_entries, mut unrecognized_paths) =
        (vec![], vec![], vec![]);
    for entry in entries {
        match entry {
            Ok(e) => {
                if let Some(info) = keymgr.describe(e.key_path()) {
                    valid_entries.push((e, info));
                } else {
                    unrecognized_paths.push(e);
                }
            }
            Err(e) => {
                unrecognized_entries.push(e);
            }
        }
    }

    // Sort the entries to make the output deterministic
    valid_entries.sort_by_key(|(e, _info)| (e.keystore_id(), e.key_path().to_string()));
    unrecognized_entries.sort_by_key(|e| e.entry().raw_id().to_string());
    unrecognized_paths.sort_by_key(|e| e.key_path().to_string());

    for entry in valid_entries {
        if args.output_format.compact {
            println!("{}", entry.0.raw_id());
        } else {
            display_entry(&entry, display_keystore_id);
            println!();
        }
    }
    println!();

    if !unrecognized_entries.is_empty() || !unrecognized_paths.is_empty() {
        println!("Broken entries\n");
        for entry in unrecognized_entries {
            display_unrecognized_entry(&entry, display_keystore_id, args.output_format.compact);
        }
        for entry in unrecognized_paths {
            let raw_id = entry.raw_id();
            if args.output_format.compact {
                println!("{raw_id}");
            } else {
                if display_keystore_id {
                    println!("Keystore ID: *not available*");
                }
                println!("Location: {raw_id}");
                println!("Error: Unrecognized\n");
            }
        }
    }
    Ok(())
}

/// Run `keys list-keystores` subcommand.
fn run_list_keystores(client: &InertTorClient) -> Result<()> {
    let keymgr = client.keymgr()?;
    let entries = keymgr.list_keystores();

    if entries.is_empty() {
        println!("Currently there are no keystores available.");
    } else {
        println!("Keystores:\n");
        for entry in entries {
            // TODO: We need something similar to [`KeyPathInfo`](tor_keymgr::KeyPathInfo)
            // for `KeystoreId`
            println!("- {:?}\n", entry.as_ref());
        }
    }

    Ok(())
}

/// Run `keys check-integrity` subcommand.
fn run_check_integrity<R: Runtime>(
    args: &CheckIntegrityArgs,
    client: &TorClient<R>,
    config: &ArtiConfig,
    client_config: &TorClientConfig,
) -> Result<()> {
    let keymgr = client.keymgr()?;

    let keystore_ids = match &args.keystore_id {
        Some(id) => vec![id.to_owned()],
        None => keymgr.list_keystores(),
    };
    let keystores: Vec<(_, Vec<KeystoreEntryResult<KeystoreEntry>>)> = keystore_ids
        .into_iter()
        .map(|id| keymgr.list_by_id(&id).map(|entries| (id, entries)))
        .collect::<Result<Vec<_>, _>>()?;

    // Unlike `keystores`, which has type `Vec<(KeystoreId, Vec<KeystoreEntryResult<KeystoreEntry>>)>`,
    // `affected_keystores` has type `InvalidKeystoreEntries`. This distinction is
    // necessary because the entries in `keystores` will be evaluated, and if any are
    // found to be invalid, the associated error messages must be stored somewhere
    // for later display.
    let mut affected_keystores = Vec::new();
    cfg_if::cfg_if! {
        if #[cfg(feature = "onion-service-service")] {
            // `service` cannot be dropped as long as `expired_entries` is in use, since
            // `expired_entries` holds references to `services`.
            let services = create_all_services(config, client_config)?;
            let mut expired_entries: Vec<_> = get_expired_keys(&services, client)?;
        }
    }

    for (id, entries) in keystores {
        let mut invalid_entries = entries
            .into_iter()
            .filter_map(|entry| match entry {
                Ok(e) => keymgr
                    .validate_entry_integrity(&e)
                    .map_err(|err| InvalidKeystoreEntry {
                        entry: Ok(e),
                        error_msg: err.to_string(),
                    })
                    .err(),
                Err(err) => {
                    let error = err.error().to_string();
                    Some(InvalidKeystoreEntry {
                        entry: Err(err),
                        error_msg: error,
                    })
                }
            })
            .collect::<Vec<_>>();

        cfg_if::cfg_if! {
            if #[cfg(feature = "onion-service-service")] {
                // For the current keystore, transfer its expired keys from `expired_entries`
                // to `invalid_entries`.
                expired_entries.retain(|expired_entry| {
                    match &expired_entry.entry {
                        Ok(entry) => {
                            if entry.keystore_id() == &id {
                                invalid_entries.push(expired_entry.clone());
                                return false;
                            }
                        }
                        Err(err) => {
                            eprintln!("WARNING: Unexpected invalid keystore entry encountered: {}", err);
                        }
                    }
                    true
                })
            }
        }

        if invalid_entries.is_empty() {
            println!("{}: OK.\n", id);
            continue;
        }

        affected_keystores.push(InvalidKeystoreEntries {
            keystore_id: id,
            entries: invalid_entries,
        });
    }

    // Expired entries are obtained from the registered keystore. Since we have iterated over every
    // registered keystore and removed all entries associated with the current keystore, the
    // collection `expired_entries` should be empty. If it is not, there is a bug (see
    // [`OnionService::list_expired_keys`]).
    cfg_if::cfg_if! {
        if #[cfg(feature = "onion-service-service")] {
            if !expired_entries.is_empty() {
                return Err(anyhow::anyhow!(
                    "Encountered an expired key that doesn't belong to a registered keystore."
                ));
            }
        }
    }

    display_invalid_keystore_entries(&affected_keystores);

    maybe_remove_invalid_entries(args, &affected_keystores, keymgr)?;

    Ok(())
}

/// Helper function for `run_check_integrity` that reduces cognitive complexity.
///
/// Displays invalid keystore entries grouped by `KeystoreId`, showing the `raw_id`
/// of each key and the associated error message in a unified report to the user.
/// If no invalid entries are provided, nothing is printed.
fn display_invalid_keystore_entries(affected_keystores: &[InvalidKeystoreEntries]) {
    if affected_keystores.is_empty() {
        return;
    }

    print_check_integrity_incipit(affected_keystores);

    for InvalidKeystoreEntries {
        keystore_id,
        entries,
    } in affected_keystores
    {
        println!("\nInvalid keystore entries in keystore {}:\n", keystore_id);
        for InvalidKeystoreEntry { entry, error_msg } in entries {
            let raw_id = match entry {
                Ok(e) => e.raw_id(),
                Err(e) => e.entry().raw_id(),
            };
            println!("{raw_id}");
            println!("\tError: {}", error_msg);
        }
    }
}

/// Helper function for `run_check_integrity`.
///
/// Creates an [`OnionService`] for each configured hidden service.
#[cfg(feature = "onion-service-service")]
fn create_all_services(
    config: &ArtiConfig,
    client_config: &TorClientConfig,
) -> Result<Vec<OnionService>> {
    let mut services = Vec::new();
    for cfg in config.onion_services.values() {
        services.push(
            TorClient::<tor_rtcompat::PreferredRuntime>::create_onion_service(
                client_config,
                cfg.svc_cfg.clone(),
            )?,
        );
    }
    Ok(services)
}

/// Helper function for `run_check_integrity`.
///
/// Gathers all expired keys from the provided hidden services.
#[cfg(feature = "onion-service-service")]
fn get_expired_keys<'a, R: Runtime>(
    services: &'a Vec<OnionService>,
    client: &TorClient<R>,
) -> Result<Vec<InvalidKeystoreEntry<'a>>> {
    let netdir = client.dirmgr()?.timely_netdir()?;

    let mut expired_keys = Vec::new();
    for service in services {
        expired_keys.append(
            &mut service
                .list_expired_keys(&netdir)?
                .into_iter()
                .map(|entry| InvalidKeystoreEntry {
                    entry: Ok(entry),
                    error_msg: "The entry is expired.".to_string(),
                })
                .collect(),
        );
    }
    Ok(expired_keys)
}

/// Helper function for `run_check_integrity`.
///
/// Removes invalid keystore entries.
/// Prints an error message if one or more entries fail to be removed.
/// Returns `Err` if an I/O error occurs.
fn maybe_remove_invalid_entries(
    args: &CheckIntegrityArgs,
    affected_keystores: &[InvalidKeystoreEntries],
    keymgr: &KeyMgr,
) -> Result<()> {
    if affected_keystores.is_empty() || !args.sweep {
        return Ok(());
    }

    let should_remove = args.batch || prompt("Remove all invalid entries?")?;

    if !should_remove {
        return Ok(());
    }

    for InvalidKeystoreEntries {
        keystore_id: _,
        entries,
    } in affected_keystores
    {
        for InvalidKeystoreEntry {
            entry,
            error_msg: _,
        } in entries.iter()
        {
            let (raw_id, keystore_id) = match entry {
                Ok(e) => (e.raw_id(), e.keystore_id()),
                Err(e) => (e.entry().raw_id(), e.entry().keystore_id()),
            };

            if keymgr
                .remove_unchecked(&raw_id.to_string(), keystore_id)
                .is_err()
            {
                eprintln!("Failed to remove entry at location: {raw_id}");
            }
        }
    }

    Ok(())
}

/// Helper function for `display_invalid_keystore_entries` that reduces cognitive complexity.
///
/// Produces and displays the opening section of the final output, given a list of keystores
/// containing invalid entries and their IDs. This function does not check whether
/// `affected_keystores` or the inner collections are empty.
fn print_check_integrity_incipit(affected_keystores: &[InvalidKeystoreEntries]) {
    let len = affected_keystores.len();

    let mut incipit = "Found problems in keystore".to_string();
    if len > 1 {
        incipit.push('s');
    }
    incipit.push_str(": ");

    let keystore_names: Vec<_> = affected_keystores
        .iter()
        .map(|x| x.keystore_id.to_string())
        .collect();
    incipit.push_str(&keystore_names.join(", "));
    incipit.push('.');

    println!("{}", incipit);
}
