-- vim: set ft=sql nonu :

-- postgres schema for all log related stuff
create schema logs;

-- writer role may create tables (partitions) and insert data
create role write_logs with nologin;
grant usage on schema logs to write_logs;
grant connect on database log to write_logs;
alter role write_logs set search_path to 'logs';
alter default privileges for role write_logs in schema logs
	grant insert on tables,
	grant usage on sequences;

-- reader role may select data, execute functions and create temporary objects
create role read_logs with nologin;
grant connect, temporary on database log to read_logs;
grant execute on all functions in schema logs to read_logs;
grant usage on schema logs to read_logs;
alter role read_logs set search_path to 'logs';
alter default privileges for role read_logs in schema logs
	grant select on tables,
	grant execute on functions;

-- users
create role stuffstream with login password 'stuffstream-password' in role read_logs;
alter role stuffstream set search_path to 'logs';
create role stuffimport with login password 'stuffimport-password' in role write_logs;
alter role stuffimport set search_path to 'logs';

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

-- stuffimport will create tables like this
-- create table logs.logs_2021_10 partition of logs for values from ('2021-10-01') to ('2021-11-01');
-- alter table logs.logs_2021_10 owner to write_logs;

create or replace function logs.to_number_or_null(text) returns numeric as $$
begin
	return cast($1 as numeric);
exception
	when invalid_text_representation then
		return null;
end;
$$ language plpgsql immutable;

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

