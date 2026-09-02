from arti_rpc_tests import arti_test
from arti_rpc import ArtiRpcError, ArtiRpcErrorStatus


@arti_test
def deferred_bootstrap(context):
    # TODO: Find a cleaner approach.
    #
    # At this point we're stopping the arti process and re-launching it
    # with 'defer_bootstrap=True'.
    context.arti_process.close(gently=True)
    context.launch_arti(extra_args=["-o", "application.defer_bootstrap=true"])

    conn = context.open_rpc_connection(require_superuser=True)
    su = conn.session().invoke("arti:get_superuser_capability")
    su = conn.make_object(su["id"])

    client = conn.session().invoke("arti:get_client")
    client = conn.make_object(client["id"])
    s = client.invoke("arti:get_client_status")
    assert not s["ready"]
    assert s["fraction"] < 0.001  # avoiding float eq
    assert "disabled" in s["blocked"]

    try:
        _ = conn.open_stream("www.torproject.org", 80)
        assert False  # Should fail since we have been told not to bootstrap.
    except ArtiRpcError as e:
        assert e.status_code() == ArtiRpcErrorStatus.STREAM_FAILED

    print(su.invoke("arti:bootstrap_client"))

    s = client.invoke("arti:get_client_status")
    assert s["fraction"] > 0.3
    # TODO: Make `ready` work better; right now it is "false" here. arti#2564
    assert s.get("blocked") is None

    # this time it should work!
    _ = conn.open_stream("www.torproject.org", 80)
