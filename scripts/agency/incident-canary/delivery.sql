SELECT json_build_object('incident_id', incident_id, 'events', count(*),
                         'all_delivered', bool_and(notified AND tracked))
FROM agency_incident_events WHERE incident_id = :'incident'::uuid GROUP BY incident_id;
