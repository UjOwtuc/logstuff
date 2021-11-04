-- vim: set ft=sql nonu :

-- postgres schema for all log related stuff
create schema logs;

-- writer role may create tables (partitions) and insert data
create role write_logs with nologin;
grant usage, create on schema logs to write_logs;
grant connect on database log to write_logs;
alter role write_logs set search_path to 'logs';
alter default privileges in schema logs grant insert on tables to write_logs;
alter default privileges in schema logs grant usage on sequences to write_logs;

-- reader role may select data, execute functions and create temporary objects
create role read_logs with nologin;
grant connect, temporary on database log to read_logs;
grant execute on all functions in schema logs to read_logs;
grant usage on schema logs to read_logs;
alter role read_logs set search_path to 'logs';
alter default privileges in schema logs grant select on tables to read_logs;
alter default privileges in schema logs grant execute on functions to read_logs;

-- users
create role stuffstream with login password 'stuffstream-password' in role read_logs;
alter role stuffstream set search_path to 'logs';
create role stuffimport with login password 'stuffimport-password' in role write_logs;
alter role stuffimport set search_path to 'logs';
create role stufftail with login password 'stufftail-password' in role read_logs;
alter role stufftail set search_path to 'logs';

create sequence logs.logs_id;

create table logs.logs(
	id integer not null default nextval('logs.logs_id'),
	tstamp timestamp with time zone not null,
	doc jsonb not null,
	search tsvector
) partition by range(tstamp);
alter table logs.logs owner to write_logs;

create index idx_logs_id_tstamp on logs.logs(id, tstamp);
create index idx_search on logs.logs using GIN(search);

-- create table logs.logs_2021_10 partition of logs.logs for values from ('2021-10-01') to ('2021-11-01');
-- alter table logs.logs_2021_10 owner to write_logs;

-- thanks to Michael Fuhr (https://www.postgresql.org/message-id/20050810133157.GA46247@winnie.fuhr.org)
CREATE FUNCTION logs.count_estimate(query text) RETURNS integer AS $$
DECLARE
    rec   record;
    rows  integer;
BEGIN
    FOR rec IN EXECUTE 'EXPLAIN ' || query LOOP
        rows := substring(rec."QUERY PLAN" FROM ' rows=([[:digit:]]+)');
        EXIT WHEN rows IS NOT NULL;
    END LOOP;



    RETURN rows;
END;
$$ LANGUAGE plpgsql VOLATILE STRICT;

